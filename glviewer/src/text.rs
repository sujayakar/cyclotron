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

impl TextCache {
    pub fn new(display: &Display, names: &HashMap<String, NameId>) -> Self {
        let font_data = include_bytes!("../resources/Inconsolata-Regular.ttf");
        let font = Font::try_from_bytes(&font_data[..]).unwrap();

        let scale = display.gl_window().window().scale_factor();
        let (cache_width, cache_height) = ((512.0 * scale) as u32, (512.0 * scale) as u32);
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

        let scale = rusttype::Scale::uniform(48. * scale as f32);
        let v_metrics = font.v_metrics(scale);
        let mut glyphs_by_name = HashMap::new();

        for (string, &name_id) in names.iter() {
            let mut glyphs = vec![];
            let mut caret = rusttype::point(0.0, v_metrics.ascent);
            let mut last_glyph_id = None;

            println!("Typesetting {}...", string);

            for c in string.chars() {
                let base_glyph = font.glyph(c);
                if let Some(id) = last_glyph_id.take() {
                    caret.x += font.pair_kerning(scale, id, base_glyph.id());
                }
                println!("  {} @ {:?}", c, caret);

                last_glyph_id = Some(base_glyph.id());
                let glyph = base_glyph.scaled(scale).positioned(caret);
                caret.x += glyph.unpositioned().h_metrics().advance_width;

                cache.queue_glyph(0, glyph.clone());
                glyphs.push(glyph);
            }
            glyphs_by_name.insert(name_id, glyphs);
        }

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

        let mut labels = HashMap::with_capacity(glyphs_by_name.len());
        for (name_id, glyphs) in glyphs_by_name {
            let mut rectangles = Vec::with_capacity(glyphs.len());

            for glyph in glyphs {
                match cache.rect_for(0, &glyph) {
                    Ok(Some(r)) => rectangles.push(r),
                    // Characters like " " don't have associated glyphs.
                    Ok(None) => continue,
                    Err(..) => panic!("Failed to find {:?}", glyph),
                };
            }

            // Normalize the text size to height 1.
            let scale = match rectangles.iter().map(|(_, r)| r.max.y).max() {
                Some(m) => 1. / (m as f32),
                None => continue,
            };

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
                vertices.extend(&[
                    TextVertex {
                        glyph: [min.x, min.y],
                        tex_coords: [uv_rect.min.x, uv_rect.min.y],
                        task_begin,
                        task_end,
                    },
                    TextVertex {
                        glyph: [max.x, min.y],
                        tex_coords: [uv_rect.max.x, uv_rect.min.y],
                        task_begin,
                        task_end,
                    },
                    TextVertex {
                        glyph: [min.x, max.y],
                        tex_coords: [uv_rect.min.x, uv_rect.max.y],
                        task_begin,
                        task_end,
                    },
                    TextVertex {
                        glyph: [max.x, max.y],
                        tex_coords: [uv_rect.max.x, uv_rect.max.y],
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
        // TODO: Properly scale the last glyph that gets truncated.
        let vertex = r#"
            #version 140

            in vec2 glyph;
            in float task_begin;
            in float task_end;
            in vec2 tex_coords;

            uniform vec2 scale;
            uniform vec2 offset;

            out vec2 v_tex_coords;

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
                v_tex_coords = tex_coords;
            }
        "#;
        let fragment = r#"
            #version 140

            uniform sampler2D tex;
            in vec2 v_tex_coords;
            out vec4 f_color;

            void main() {
                f_color = vec4(0.0, 0.0, 0.0, texture(tex, v_tex_coords).r);
                // f_color = vec4(0.0, 0.0, 0.0, 1.0);
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
    tex_coords: [f32; 2],
}
implement_vertex!(TextVertex, glyph, task_begin, task_end, tex_coords);

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
                .minify_filter(MinifySamplerFilter::Nearest)
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
