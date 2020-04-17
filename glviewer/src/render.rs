use crate::layout::GroupId;
use crate::view::View;
use crate::db::{Span, NameId};
use std::collections::HashMap;
use crate::layout::{Layout, BoxListKey, SpanRange};
use glium::{
    Surface,
    Display,
    Program,
    Frame,
    Depth,
    Blend,
    implement_vertex,
    uniform,
    index::{
        PrimitiveType,
        IndexBuffer
    },
    vertex::VertexBuffer,
    draw_parameters::DepthTest,
    texture::Texture1d,
    DrawParameters,
};

#[derive(Copy, Clone)]
struct SimpleBoxVertex {
    position: [f32; 2],
}
implement_vertex!(SimpleBoxVertex, position);

#[derive(Copy, Clone)]
struct BoxListVertex {
    position: [f32; 2],
    group_ident: i32,
}
implement_vertex!(BoxListVertex, position, group_ident);

struct SimpleBoxData {
    vertex: VertexBuffer<SimpleBoxVertex>,
    // just a triangle fan, no need for index data
}

impl SimpleBoxData {
    fn new(display: &Display) -> SimpleBoxData {
        let vertex = VertexBuffer::new(display, &[
            SimpleBoxVertex { position: [0.0, 0.0] },
            SimpleBoxVertex { position: [1.0, 0.0] },
            SimpleBoxVertex { position: [0.0, 1.0] },
            SimpleBoxVertex { position: [1.0, 1.0] },
        ]).unwrap();

        SimpleBoxData {
            vertex,
        }
    }

    fn draw(
        &self,
        shaders: &Shaders,
        params: &DrawParameters,
        target: &mut Frame,
        color: Color,
        region: SimpleRegion,
    ) {
        /*
            left = offset * scale
            right = scale + offset * scale

            right - left = scale

            left / (right - left) = offset

        */

        target.draw(
            &self.vertex,
            glium::index::NoIndices(PrimitiveType::TriangleStrip),
            &shaders.simple_box_program,
            &uniform! {
                scale: [
                    region.right - region.left,
                    region.bottom - region.top,
                ],
                offset: [
                    region.left / (region.right - region.left),
                    region.top / (region.bottom - region.top),
                ],
                item_color: [color.r, color.g, color.b, color.a],
            },
            &params).unwrap();
    }
}

struct BoxListData {
    vertex: VertexBuffer<BoxListVertex>,
    index: IndexBuffer<u32>,
}

impl BoxListData {
    fn from_iter(display: &Display, spans: impl Iterator<Item=(GroupId, NameId, Span)>) -> BoxListData {
        let mut verts = Vec::new();
        let mut tris = Vec::<u32>::new();

        for (_group, name, span) in spans {
            let group_ident = name.0 as i32;
            let s = verts.len() as u32;
            tris.extend(&[s, s+1, s+2, s+1, s+2, s+3]);

            verts.push(BoxListVertex { position: [(span.begin as f32) / 1e9, 0.0], group_ident });
            verts.push(BoxListVertex { position: [(span.end as f32) / 1e9, 0.0], group_ident });
            verts.push(BoxListVertex { position: [(span.begin as f32) / 1e9, 1.0], group_ident });
            verts.push(BoxListVertex { position: [(span.end as f32) / 1e9, 1.0], group_ident });
        }

        let vertex = VertexBuffer::new(display, &verts).unwrap();
        let index = IndexBuffer::new(display, PrimitiveType::TrianglesList, &tris).unwrap();

        BoxListData {
            vertex,
            index,
        }
    }

    fn draw(
        &self,
        shaders: &Shaders,
        params: &DrawParameters,
        target: &mut Frame,
        range: SpanRange,
        color_texture: &Texture1d,
        color: Color,
        name: NameId,
        highlight: Color,
        region: Region,
    ) {
        target.draw(
            &self.vertex,
            &self.index.slice(6*range.begin .. 6*range.end).unwrap(),
            &shaders.box_list_program,
            &uniform! {
                scale: [
                    1.0 / (region.logical_limit - region.logical_base),
                    region.vertical_limit - region.vertical_base,
                ],
                offset: [
                    -region.logical_base,
                    region.vertical_base / (region.vertical_limit - region.vertical_base),
                ],
                highlight_group: name.0 as i32,
                item_color: [color.r, color.g, color.b, color.a ],
                group_color: [highlight.r, highlight.g, highlight.b, highlight.a],
                color_texture: color_texture,
            },
            &params).unwrap();
    }
}

struct Shaders {
    simple_box_program: Program,
    box_list_program: Program,
}

impl Shaders {
    fn new(display: &Display) -> Shaders {
        let simple_box_program = {
            let vertex = r#"
                #version 150
                in vec2 position;
                uniform vec2 scale;
                uniform vec2 offset;

                void main() {
                    vec2 pos0 = (position + offset)*scale;
                    vec2 pos0_offset = pos0 - 0.5;
                    gl_Position = vec4(2*pos0_offset.x, -2*pos0_offset.y, 0.0, 1.0);
                }
            "#;

            let fragment = r#"
                #version 140
                uniform vec4 item_color;
                out vec4 color;
                void main() {
                    color = item_color;
                }
            "#;
            Program::from_source(display, vertex, fragment, None).unwrap()
        };

        let box_list_program = {
            let vertex = r#"
                #version 150
                in vec2 position;
                in int group_ident;

                uniform vec4 group_color;
                uniform vec4 item_color;
                uniform sampler1D color_texture;
                uniform vec2 scale;
                uniform vec2 offset;
                uniform int highlight_group;
                
                out vec4 vert_color;
                
                void main() {
                    vec2 pos0 = (position + offset)*scale;
                    vec2 pos0_offset = pos0 - 0.5;
                    gl_Position = vec4(2*pos0_offset.x, -2*pos0_offset.y, 0.0, 1.0);

                    if(highlight_group == group_ident) {
                        vert_color = group_color;
                    } else {
                        vert_color = vec4(
                            item_color.rgb * item_color.a + 
                            texelFetch(color_texture, group_ident, 0).rgb * (1 - item_color.a),
                            1.0);
                    }
                }
            "#;

            let fragment = r#"
                #version 140
                in vec4 vert_color;
                out vec4 color;
                void main() {
                    color = vert_color;
                }
            "#;

            Program::from_source(display, vertex, fragment, None).unwrap()
        };

        Shaders {
            simple_box_program,
            box_list_program,
        }
    }
}

#[derive(Copy, Clone)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

#[derive(Debug, Copy, Clone)]
pub struct SimpleRegion {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

#[derive(Debug, Copy, Clone)]
pub struct Region {
    pub vertical_base: f32,
    pub vertical_limit: f32,

    pub logical_base: f32,
    pub logical_limit: f32,
}

#[derive(Copy, Clone)]
pub enum DrawCommand {
    #[allow(unused)]
    SimpleBox {
        color: Color,
        region: SimpleRegion,
    },
    BoxList {
        key: BoxListKey,
        range: SpanRange,
        color: Color,
        name: Option<NameId>,
        highlight: Color,
        region: Region,
    },
}

pub struct RenderState {
    simple_box: SimpleBoxData,
    color_texture: Texture1d,
    shaders: Shaders,
    box_lists: HashMap<BoxListKey, BoxListData>,
}

impl RenderState {
    pub fn new(layout: &Layout, display: &Display) -> RenderState {
        let mut box_lists = HashMap::new();

        for (key, items) in layout.iter_box_lists() {
            box_lists.insert(key, BoxListData::from_iter(display, items));
        }

        let mut colors = Vec::new();

        let mut rng = rand::thread_rng();
        use rand::Rng;
        for _ in 0..256 {
            let (r, g, b) = hsl_to_rgb(
                rng.gen_range(0.0, 1.0),
                rng.gen_range(0.2, 0.8),
                rng.gen_range(0.2, 0.5));

            colors.push((r, g, b, 1.0));
        }

        let color_texture = Texture1d::new(display, colors).unwrap();

        RenderState {
            simple_box: SimpleBoxData::new(display),
            color_texture,
            shaders: Shaders::new(display),
            box_lists,
        }
    }

    pub fn draw(&self, view: &View, target: &mut Frame) {
        let params = DrawParameters {
            depth: Depth {
                test: DepthTest::Overwrite,
                write: true,
                .. Default::default()
            },
            blend: Blend::alpha_blending(),
            .. Default::default()
        };

        for cmd in view.draw_commands() {
            match cmd {
                DrawCommand::SimpleBox { color, region } => {
                    self.simple_box.draw(&self.shaders, &params, target, color, region);
                }
                DrawCommand::BoxList { key, range, color, name, highlight, region } => {
                    let data = &self.box_lists[&key];
                    data.draw(
                        &self.shaders,
                        &params,
                        target,
                        range,
                        &self.color_texture,
                        color,
                        name.unwrap_or(NameId(0xefffffff)),
                        highlight,
                        region);
                }
            }
        }
    }
}

fn hue_to_p(p: f32, q: f32, mut t: f32) -> f32 {
    if t <0.00 {
        t += 1.0;
    }
    if t > 1.0 {
        t -= 1.0;
    }
    if t < 1.0/6.0 {
        return p + (q - p) * 6.0 * t;
    }
    if t < 1.0/2.0 {
        return q;
    }
    if t < 2.0/3.0 {
        return p + (q - p) * (2.0/3.0 - t) * 6.0;
    }
    p
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    if s == 0.0 {
        (l, l, l)
    } else {
        let q = if l < 0.5 {
            l * (1.0 + s)
        } else {
            l + s - l * s
        };

        let p = 2.0 * l - q;

        (
            hue_to_p(p, q, h + 1.0/3.0),
            hue_to_p(p, q, h),
            hue_to_p(p, q, h - 1.0/3.0),
        )
    }
}