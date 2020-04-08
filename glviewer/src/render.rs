use crate::view::View;
use crate::db::Span;
use std::collections::HashMap;
use crate::layout::{Layout, BoxListKey};
use glium::{
    glutin,
    Surface,
    Display,
    Program,
    Frame,
    implement_vertex,
    uniform,
    index::{
        PrimitiveType,
        IndexBuffer
    },
    vertex::VertexBuffer,
    draw_parameters::DepthTest,
};

#[derive(Copy, Clone)]
struct SimpleBoxVertex {
    position: [f32; 2],
}
implement_vertex!(SimpleBoxVertex, position);

#[derive(Copy, Clone)]
struct BoxListVertex {
    position: [f32; 2],
    // parent_ident: u32,
}
implement_vertex!(BoxListVertex, position);

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

    fn draw() {
        // target.draw(&self.plain_rect, glium::index::NoIndices(PrimitiveType::TriangleStrip), &program,
        //     &uniform! { scale: scale_vec, offset: offset_vec, item_color: [0.9f32, 0.9, 0.9, 1.0] },
        //     &params).unwrap();
    }
}

struct BoxListData {
    vertex: VertexBuffer<BoxListVertex>,
    index: IndexBuffer<u32>,
}

impl BoxListData {
    fn from_iter(display: &Display, spans: impl Iterator<Item=Span>) -> BoxListData {
        let mut verts = Vec::new();
        let mut tris = Vec::<u32>::new();

        for span in spans {
            let s = verts.len() as u32;
            tris.extend(&[s, s+1, s+2, s+1, s+2, s+3]);

            verts.push(BoxListVertex { position: [(span.begin as f32) / 1_000_000_000.0, 0.0] });
            verts.push(BoxListVertex { position: [(span.end as f32) / 1_000_000_000.0, 0.0] });
            verts.push(BoxListVertex { position: [(span.begin as f32) / 1_000_000_000.0, 1.0] });
            verts.push(BoxListVertex { position: [(span.end as f32) / 1_000_000_000.0, 1.0] });
        }

        let vertex = VertexBuffer::new(display, &verts).unwrap();
        let index = IndexBuffer::new(display, PrimitiveType::TrianglesList, &tris).unwrap();

        BoxListData {
            vertex,
            index,
        }
    }

    fn draw() {
        // target.draw(&lane.vertex, &lane.index, &program,
        //             &uniform! {
        //                 scale: scale_vec,
        //                 offset: offset_vec,
        //                 item_color: [color.0, color.1, color.2, 1.0],
        //                 parent_color: [1.0f32, 0.0, 1.0, 1.0],
        //                 group_color: [0.0f32, 1.0, 0.0, 1.0],
        //                 highlight_item: highlight_item,
        //             },
        //             &params).unwrap();
    }
}

pub struct StaticRenderData {
    simple_box: SimpleBoxData,
    simple_box_program: Program,
    box_list_program: Program,
}

impl StaticRenderData {
    pub fn new(display: &Display) -> StaticRenderData {
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
                in uint parent_ident;
                in uint group_ident;
                in vec2 position;

                uniform vec4 parent_color;
                uniform vec4 group_color;
                uniform vec4 item_color;
                uniform vec2 scale;
                uniform vec2 offset;
                uniform uint highlight_item;
                uniform uint highlight_group;
                
                out vec4 vert_color;
                
                void main() {
                    vec2 pos0 = (position + offset)*scale;
                    vec2 pos0_offset = pos0 - 0.5;
                    gl_Position = vec4(2*pos0_offset.x, -2*pos0_offset.y, 0.0, 1.0);

                    if(highlight_item == parent_ident) {
                        vert_color = parent_color;
                    } else if(highlight_group == group_ident) {
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

        StaticRenderData {
            simple_box: SimpleBoxData::new(display),
            simple_box_program,
            box_list_program,
        }
    }
}

pub struct RenderState {
    static_data: StaticRenderData,
    box_lists: HashMap<BoxListKey, BoxListData>,
}

impl RenderState {
    pub fn new(static_data: StaticRenderData, layout: &Layout, display: &Display) -> RenderState {
        let mut box_lists = HashMap::new();

        for (key, items) in layout.iter_box_lists() {
            box_lists.insert(key, BoxListData::from_iter(display, items));
        }

        RenderState {
            static_data,
            box_lists,
        }
    }

    pub fn draw(&self, view: &View, target: &mut Frame) {

    }
}
