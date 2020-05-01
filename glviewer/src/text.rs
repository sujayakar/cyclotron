use std::borrow::Cow;
use std::collections::HashMap;

use crate::db::{Span, NameId};
use crate::render::Region;
use glium::{
    Display,
    DrawParameters,
    Frame,
    Program,
    Surface,
    VertexBuffer,
    implement_vertex,
    uniform,
};
use glium::index::{IndexBuffer, PrimitiveType};
use glium::uniforms::{MinifySamplerFilter, MagnifySamplerFilter};
use glium::texture::{
    ClientFormat,
    MipmapsOption,
    RawImage2d,
    Texture2d,
    UncompressedFloatFormat,
};
use rusttype::gpu_cache::Cache;
use rusttype::{Rect, Vector};
use rusttype::Font;

struct ScaledGlyph {
    min: Vector<f32>,
    max: Vector<f32>,
    uv_rect: Rect<f32>,
}

pub struct TextCache {
    labels: HashMap<NameId, Vec<ScaledGlyph>>,
    texture: Texture2d,
    program: Program,
}

// Texture size (before scale factor)
const CACHE_SIZE: f64 = 512.;

// Pixels per character (before scale factor)
const FONT_SCALING: f32 = 48.;

// Initial horizontal position for typesetting. (I'll be honest, I just played with this until it
// looked nice.)
const LABEL_LEFT_PADDING: f32 = 12.;

// Use 3/4 of the rectangle for the label height.
const LABEL_LINE_HEIGHT: f32 = 0.75;

impl TextCache {
    pub fn new(display: &Display, names: &HashMap<String, NameId>) -> Self {
        let font_data = include_bytes!("../resources/Inconsolata-Regular.ttf");
        let font = Font::try_from_bytes(&font_data[..]).unwrap();

        let scale = display.gl_window().window().scale_factor();
        let (cache_width, cache_height) = ((CACHE_SIZE * scale) as u32, (CACHE_SIZE * scale) as u32);
        let mut cache: Cache<'static> = Cache::builder()
            .dimensions(cache_width, cache_height)
            .build();

        // TODO: Generate mipmaps to avoid aliasing when text is very small.
        let texture = Texture2d::with_format(
            display,
            RawImage2d {
                data: Cow::Owned(vec![0u8; cache_width as usize * cache_height as usize]),
                width: cache_width,
                height: cache_height,
                format: ClientFormat::U8,
            },
            UncompressedFloatFormat::U8,
            MipmapsOption::NoMipmap,
        ).unwrap();

        let scale = rusttype::Scale::uniform(FONT_SCALING * scale as f32);
        let v_metrics = font.v_metrics(scale);
        let mut glyphs_by_name = HashMap::new();

        // First, have rusttype typeset all of our strings, creating a list of `PositionedGlyphs`
        // per input string. Emit these to its GPU cache so it can accumulate them into a texture.
        for (string, &name_id) in names.iter() {
            let mut glyphs = vec![];
            let mut caret = rusttype::point(LABEL_LEFT_PADDING, v_metrics.ascent);
            let mut last_glyph_id = None;

            for c in string.chars() {
                let base_glyph = font.glyph(c);
                if let Some(id) = last_glyph_id.take() {
                    caret.x += font.pair_kerning(scale, id, base_glyph.id());
                }
                last_glyph_id = Some(base_glyph.id());
                let glyph = base_glyph.scaled(scale).positioned(caret);
                caret.x += glyph.unpositioned().h_metrics().advance_width;

                cache.queue_glyph(0, glyph.clone());
                glyphs.push(glyph);
            }
            glyphs_by_name.insert(name_id, glyphs);
        }

        // Build the texture of all unique glyphs within our input strings.
        cache.cache_queued(|rect, data| {
            texture.main_level().write(
                glium::Rect {
                    left: rect.min.x,
                    bottom: rect.min.y,
                    width: rect.width(),
                    height: rect.height(),
                },
                RawImage2d {
                    data: Cow::Borrowed(data),
                    width: rect.width(),
                    height: rect.height(),
                    format: ClientFormat::U8,
                },
            );
        }).unwrap();

        // Pull out the positions of each glyph and their indices into the texture buffer. We want
        // to scale the text to be of length `LABEL_LINE_HEIGHT`, so also keep track of the smallest
        // and largest y values we see.
        let mut rectangles_by_name_id = HashMap::with_capacity(glyphs_by_name.len());
        let mut min_y = std::i32::MAX;
        let mut max_y = 0;
        for (name_id, glyphs) in glyphs_by_name {
            let mut rectangles = Vec::with_capacity(glyphs.len());
            for glyph in glyphs {
                let (uv_rect, screen_rect) = match cache.rect_for(0, &glyph) {
                    Ok(Some(r)) => r,
                    // Characters like " " don't have associated glyphs.
                    Ok(None) => continue,
                    Err(..) => panic!("Failed to find {:?}", glyph),
                };
                min_y = std::cmp::min(min_y, screen_rect.min.y);
                max_y = std::cmp::max(max_y, screen_rect.max.y);
                rectangles.push((uv_rect, screen_rect));
            }
            rectangles_by_name_id.insert(name_id, rectangles);
        }
        assert!(min_y < max_y);
        // TODO: This isn't typographically correct. We should be using `VMetrics` somehow.
        let scale = LABEL_LINE_HEIGHT / (max_y - min_y) as f32;

        // Now that we have the scale, do another pass to scale each rectangle and compute the final
        // result to pass to `data` below.
        let mut labels = HashMap::with_capacity(rectangles_by_name_id.len());
        for (name_id, rectangles) in rectangles_by_name_id {
            let mut scaled_glyphs = Vec::with_capacity(rectangles.len());
            for (uv_rect, screen_rect) in rectangles {
                let min = Vector { x: screen_rect.min.x as f32, y: screen_rect.min.y as f32 } * scale;
                let max = Vector { x: screen_rect.max.x as f32, y: screen_rect.max.y as f32 } * scale;
                scaled_glyphs.push(ScaledGlyph { min, max, uv_rect });
            }
            labels.insert(name_id, scaled_glyphs);
        }

        Self { labels, texture, program: Self::program(display) }
    }

    pub fn data(&self, display: &Display, labels: impl Iterator<Item=(NameId, Span)>) -> LabelListData {
        let mut vertices = vec![];
        let mut triangles = vec![];

        for (name_id, span) in labels {
            for ScaledGlyph { min, max, uv_rect } in self.labels.get(&name_id).unwrap() {
                let s = vertices.len() as u32;
                let task_begin = (span.begin as f32) / 1e9;
                let task_end = (span.end as f32) / 1e9;

                let tex_base = [uv_rect.min.x, uv_rect.min.y];
                let tex_dimensions = [uv_rect.max.x - uv_rect.min.x, uv_rect.max.y - uv_rect.min.y];

                vertices.extend(&[
                    TextVertex {
                        glyph: [min.x, min.y],

                        tex_pos: [0., 0.],
                        tex_base,
                        tex_dimensions,

                        task_begin,
                        task_end,
                    },
                    TextVertex {
                        glyph: [max.x, min.y],

                        tex_pos: [1., 0.],
                        tex_base,
                        tex_dimensions,

                        task_begin,
                        task_end,
                    },
                    TextVertex {
                        glyph: [min.x, max.y],

                        tex_pos: [0., 1.],
                        tex_base,
                        tex_dimensions,

                        task_begin,
                        task_end,
                    },
                    TextVertex {
                        glyph: [max.x, max.y],

                        tex_pos: [1., 1.],
                        tex_base,
                        tex_dimensions,

                        task_begin,
                        task_end,
                    },
                ]);

                triangles.extend(&[s, s+1, s+2, s+1, s+2, s+3]);
            }
        }

        let vertex_buffer = VertexBuffer::new(display, &vertices).unwrap();
        let index_buffer = IndexBuffer::new(
            display,
            PrimitiveType::TrianglesList,
            &triangles,
        ).unwrap();
        LabelListData { vertex_buffer, index_buffer }
    }

    fn program(display: &Display) -> Program {
        let vertex = r#"
            #version 140

            in vec2 glyph;
            in float task_begin;
            in float task_end;
            in vec2 tex_pos;
            in vec2 tex_base;
            in vec2 tex_dimensions;

            uniform vec2 scale;
            uniform vec2 offset;

            out vec2 v_tex_pos;
            out vec2 v_tex_base;
            out vec2 v_tex_dimensions;

            void main() {
                // Compute the left and right coordinates of the task.
                float task_left = (task_begin + offset.x) * scale.x;
                float task_right = (task_end + offset.x) * scale.x;

                // Compute the bottom left corner of the task, clamping to the left side of the screen.
                vec2 local_origin = vec2(max(task_left, 0.), offset.y * scale.y);

                // Add our glyph vector to the corner, scaling uniformly by the vertical scaling.
                vec2 glyph0 = local_origin + (glyph * vec2(scale.y, scale.y));

                // Clamp the x-coordinate if past the end of the task.
                vec2 glyph1 = vec2(min(glyph0.x, task_right), glyph0.y);
                vec2 glyph1_offset = glyph1 - 0.5;
                gl_Position = vec4(2 * glyph1_offset.x, -2 * glyph1_offset.y, 0.0, 1.0);

                // Do the same to our texture coordinate.
                v_tex_pos = vec2(tex_pos.x * (glyph1.x / glyph0.x), tex_pos.y);
                v_tex_base = tex_base;
                v_tex_dimensions = tex_dimensions;
            }
        "#;
        let fragment = r#"
            #version 140

            uniform sampler2D tex;
            in vec2 v_tex_pos;
            in vec2 v_tex_base;
            in vec2 v_tex_dimensions;

            out vec4 f_color;

            // Shrink the glyph by 5% so we have room for our border.
            float scaled_sample(vec2 pos) {
                vec2 scaled = (pos - 0.025) / 0.95;
                float mask = 1.0;

                if (scaled.x < 0.0 || scaled.x > 1.0 || scaled.y < 0.0 || scaled.y > 1.0)
                    mask = 0.0;

                return texture(tex, v_tex_base + scaled * v_tex_dimensions).r * mask;
            }

            void main() {
                float center = scaled_sample(v_tex_pos);

                // Convolve with
                //
                //   1 1 1
                //   1 0 1
                //   1 1 1
                //
                // to compute a border weight that we then clamp at 1.0.
                //
                // TODO: This doesn't look great when the text is small. Ideally we should mipmap
                // the font texture and precompute the outline within the texture.
                float width = 0.02;
                float neighbors = -center;
                for (int i = -1; i <= 1; i++) {
                    for (int j = -1; j <= 1; j++) {
                        neighbors += scaled_sample(v_tex_pos + vec2(i * width, j * width));
                    }
                }

                // Clamp the border's contribution when this pixel is actually on at its center
                // point so we don't try to render a border on the interior of the glyph.
                float border = min(1.0 - center, neighbors) * 0.5;

                f_color = vec4(vec3(1.0, 1.0, 1.0) * center + vec3(0.05, 0.05, 0.05) * border, max(center, border));
            }
        "#;
        Program::from_source(display, vertex, fragment, None).unwrap()
    }
}

#[derive(Copy, Clone)]
struct TextVertex {
    glyph: [f32; 2],
    task_begin: f32,
    task_end: f32,

    tex_pos: [f32; 2],
    tex_base: [f32; 2],
    tex_dimensions: [f32; 2],
}
implement_vertex!(TextVertex, glyph, task_begin, task_end, tex_pos, tex_base, tex_dimensions);

pub struct LabelListData {
    vertex_buffer: VertexBuffer<TextVertex>,
    index_buffer: IndexBuffer<u32>,
}

impl LabelListData {
    pub fn draw(&self, text_cache: &TextCache, params: &DrawParameters, target: &mut Frame, region: Region) {
        let uniforms = uniform! {
            scale: [
                1.0 / (region.logical_limit - region.logical_base),
                region.vertical_limit - region.vertical_base,
            ],
            offset: [
                -region.logical_base,
                region.vertical_base / (region.vertical_limit - region.vertical_base),
            ],
            tex: text_cache.texture
                .sampled()
                .magnify_filter(MagnifySamplerFilter::Linear)
                .minify_filter(MinifySamplerFilter::Linear)
        };
        target.draw(
            &self.vertex_buffer,
            &self.index_buffer,
            &text_cache.program,
            &uniforms,
            params,
        ).unwrap();
    }
}

#[test]
fn test_typesetting() {
    let font_data = include_bytes!("../resources/Inconsolata-Regular.ttf");
    let font = Font::try_from_bytes(&font_data[..]).unwrap();
    let scale = 2.0;
    let (cache_width, cache_height) = ((512.0 * scale) as u32, (512.0 * scale) as u32);
    let mut cache: Cache<'static> = Cache::builder()
            .dimensions(cache_width, cache_height)
            .build();
    let scale = rusttype::Scale::uniform(48. * scale as f32);
    let v_metrics = font.v_metrics(scale);
    dbg!(v_metrics);

    let mut caret = rusttype::point(0.0, v_metrics.ascent);
    let mut last_glyph_id = None;
    let mut glyphs = vec![];

    println!("Typesetting...");

    for c in "fg".chars() {
        let base_glyph = font.glyph(c);
        if let Some(id) = last_glyph_id.take() {
            caret.x += font.pair_kerning(scale, id, base_glyph.id());
        }
        println!("{}: {:?}", c, caret);

        last_glyph_id = Some(base_glyph.id());
        let glyph = base_glyph.scaled(scale).positioned(caret);
        caret.x += glyph.unpositioned().h_metrics().advance_width;
        cache.queue_glyph(0, glyph.clone());
        glyphs.push((c, glyph));
    }

    println!("\nCaching to texture...");

    cache.cache_queued(|_, _| {}).unwrap();

    for (c, glyph) in glyphs {
        if let Some((_uv_rect, screen_rect)) = cache.rect_for(0, &glyph).unwrap() {
            println!("{}: {:?}", c, screen_rect);
        } else {
            println!("{}: no rect", c);
        }
    }
}
