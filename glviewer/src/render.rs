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
    implement_vertex,
    uniform,
    index::{
        PrimitiveType,
        IndexBuffer
    },
    vertex::VertexBuffer,
    draw_parameters::DepthTest,
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
    group_ident: u32,
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
    fn from_iter(display: &Display, spans: impl Iterator<Item=(NameId, Span)>) -> BoxListData {
        let mut verts = Vec::new();
        let mut tris = Vec::<u32>::new();

        for (name, span) in spans {
            let group_ident = name.0;
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
        color: Color,
        name_highlight: Option<(NameId, Color)>,
        region: Region,
    ) {

        /*
        base = 100
        limit = 105

        1 = limit*scale + offset*scale
        0 = base*scale + offset*scale

        1 = (limit-base)*scale

        limit-base = scale
        */
        let group = name_highlight.map(|n| (n.0).0).unwrap_or(0);
        let group_color = name_highlight.map(|n| n.1).unwrap_or(color);

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
                item_color: [color.r, color.g, color.b, color.a],
                highlight_group: group,
                group_color: [group_color.r, group_color.g, group_color.b, group_color.a],
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
                in uint group_ident;

                uniform vec4 group_color;
                uniform vec4 item_color;
                uniform vec2 scale;
                uniform vec2 offset;
                uniform uint highlight_group;
                
                out vec4 vert_color;
                
                void main() {
                    vec2 pos0 = (position + offset)*scale;
                    vec2 pos0_offset = pos0 - 0.5;
                    gl_Position = vec4(2*pos0_offset.x, -2*pos0_offset.y, 0.0, 1.0);

                    if(highlight_group == group_ident) {
                        vert_color = group_color;
                    } else {
                        vert_color = item_color;
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
        name_highlight: Option<(NameId, Color)>,
        region: Region,
    },
}

pub struct RenderState {
    simple_box: SimpleBoxData,
    shaders: Shaders,
    box_lists: HashMap<BoxListKey, BoxListData>,
}

impl RenderState {
    pub fn new(layout: &Layout, display: &Display) -> RenderState {
        let mut box_lists = HashMap::new();

        for (key, items) in layout.iter_box_lists() {
            box_lists.insert(key, BoxListData::from_iter(display, items));
        }

        RenderState {
            simple_box: SimpleBoxData::new(display),
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
            .. Default::default()
        };

        for cmd in view.draw_commands() {
            match cmd {
                DrawCommand::SimpleBox { color, region } => {
                    self.simple_box.draw(&self.shaders, &params, target, color, region);
                }
                DrawCommand::BoxList { key, range, color, name_highlight, region } => {
                    let data = &self.box_lists[&key];
                    data.draw(&self.shaders, &params, target, range, color, name_highlight, region);
                }
            }
        }
    }
}