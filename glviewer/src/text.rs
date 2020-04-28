use std::borrow::Cow;
use glium::{
    Blend,
    Display,
    DrawParameters,
    Frame,
    Program,
    Rect,
    Surface,
    VertexBuffer,
    implement_vertex,
    uniform,
};
use glium::index::{IndexBuffer, PrimitiveType};
use glium::uniforms::MagnifySamplerFilter;
use glium::texture::{
    ClientFormat,
    MipmapsOption,
    RawImage2d,
    Texture2d,
    UncompressedFloatFormat,
};
use rusttype::gpu_cache::Cache;
use rusttype::Font;

#[derive(Copy, Clone)]
struct TextVertex {
    position: [f32; 2],
    tex_coords: [f32; 2],
}
implement_vertex!(TextVertex, position, tex_coords);

pub struct FontCache {
    cache_texture: Texture2d,
    vertex_buffer: VertexBuffer<TextVertex>,
    index_buffer: IndexBuffer<u32>,
    program: Program,
}

impl FontCache {
    pub fn new(display: &Display) -> Self {
        let font_data = include_bytes!("../resources/Inconsolata-Regular.ttf");
        let font = Font::try_from_bytes(&font_data[..]).unwrap();
        let scale = display.gl_window().window().scale_factor();
        let (cache_width, cache_height) = ((512.0 * scale) as u32, (512.0 * scale) as u32);
        let mut cache: Cache<'static> = Cache::builder()
            .dimensions(cache_width, cache_height)
            .build();
        let cache_texture = Texture2d::with_format(
            display,
            RawImage2d {
                data: Cow::Owned(vec![128u8; cache_width as usize * cache_height as usize]),
                width: cache_width,
                height: cache_height,
                format: ClientFormat::U8,
            },
            UncompressedFloatFormat::U8,
            MipmapsOption::NoMipmap,
        ).unwrap();

        let test = "render me pls";
        let scale = rusttype::Scale::uniform(24.0 * scale as f32);

        let v_metrics = font.v_metrics(scale);
        let mut caret = rusttype::point(0.0, v_metrics.ascent);
        let mut last_glyph_id = None;
        let mut glyphs = vec![];
        for c in test.chars() {
            let base_glyph = font.glyph(c);
            if let Some(id) = last_glyph_id.take() {
                caret.x += font.pair_kerning(scale, id, base_glyph.id());
            }
            last_glyph_id = Some(base_glyph.id());
            let glyph = base_glyph.scaled(scale).positioned(caret);
            caret.x += glyph.unpositioned().h_metrics().advance_width;
            glyphs.push(glyph);
        }
        for glyph in &glyphs {
            cache.queue_glyph(0, glyph.clone());
        }
        cache.cache_queued(|rect, data| {
            cache_texture.main_level().write(
                Rect {
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

        let program = {
            let vertex = r#"
            #version 140

            in vec2 position;
            in vec2 tex_coords;

            out vec2 v_tex_coords;

            void main() {
                gl_Position = vec4(position, 0.0, 1.0);
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
            }
        "#;
            Program::from_source(display, vertex, fragment, None).unwrap()
        };
        let (screen_width, screen_height) = {
            let (w, h) = display.get_framebuffer_dimensions();
            (w as f32, h as f32)
        };
        let origin = rusttype::point(0.0, 0.0);

        let mut vertices = vec![];
        let mut triangles = vec![];

        for glyph in &glyphs {
            let (uv_rect, screen_rect) = match cache.rect_for(0, glyph) {
                Ok(Some(r)) => r,
                Ok(None) | Err(..) => continue,
            };
            let min_v = rusttype::vector(
                screen_rect.min.x as f32 / screen_width - 0.5,
                1.0 - screen_rect.min.y as f32 / screen_height - 0.5,
            );
            let max_v = rusttype::vector(
                screen_rect.max.x as f32 / screen_width - 0.5,
                1.0 - screen_rect.max.y as f32 / screen_height - 0.5,
            );
            let gl_rect = rusttype::Rect {
                min: origin + min_v * 2.0,
                max: origin + max_v * 2.0,
            };

            let s = vertices.len() as u32;
            vertices.extend(&[
                TextVertex {
                    position: [gl_rect.min.x, gl_rect.min.y],
                    tex_coords: [uv_rect.min.x, uv_rect.min.y],
                },
                TextVertex {
                    position: [gl_rect.max.x, gl_rect.min.y],
                    tex_coords: [uv_rect.max.x, uv_rect.min.y],
                },
                TextVertex {
                    position: [gl_rect.min.x, gl_rect.max.y],
                    tex_coords: [uv_rect.min.x, uv_rect.max.y],
                },
                TextVertex {
                    position: [gl_rect.max.x, gl_rect.max.y],
                    tex_coords: [uv_rect.max.x, uv_rect.max.y],
                },
            ]);
            triangles.extend(&[s, s+1, s+2, s+1, s+2, s+3]);
        }
        let vertex_buffer = VertexBuffer::new(display, &vertices).unwrap();
        let index_buffer = IndexBuffer::new(
            display,
            PrimitiveType::TrianglesList,
            &triangles,
        ).unwrap();

        FontCache { vertex_buffer, index_buffer, cache_texture, program }
    }

    pub fn draw(&self, target: &mut Frame) {
        let uniforms = uniform! {
            tex: self.cache_texture
                .sampled()
                .magnify_filter(MagnifySamplerFilter::Nearest)
        };
        target.draw(
            &self.vertex_buffer,
            &self.index_buffer,
            &self.program,
            &uniforms,
            &DrawParameters {
                blend: Blend::alpha_blending(),
                ..Default::default()
            },
        ).unwrap();
    }
}
