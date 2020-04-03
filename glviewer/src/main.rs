use glium::{glutin, Surface, implement_vertex, uniform, index::PrimitiveType};
use structopt::StructOpt;
use cyclotron_backend::TraceEvent;
use std::io::{BufReader, BufRead};
use std::fs::File;
use std::collections::{HashMap, HashSet};

#[derive(Debug, StructOpt)]
struct Args {
    trace: String,
    // grep: Vec<String>,
    // hide_wakeups: Vec<String>,
}

fn main() {
    let args = Args::from_args();
    
    let mut file = BufReader::new(File::open(&args.trace).unwrap());
    let mut events = Vec::new();

    loop {
        let mut buf = String::new();
        let num_read = file.read_line(&mut buf).unwrap();

        if num_read == 0 || !buf.ends_with("\n") {
            break;
        } else {
            buf.pop();
            let event: TraceEvent = serde_json::from_str(&buf).unwrap();
            events.push(event);
        }
    }

    let mut spans = HashMap::new();
    let mut parents = HashSet::new();

    for event in events {
        match event {
            TraceEvent::AsyncStart { id, ts, .. } |
            TraceEvent::AsyncOnCPU { id, ts, .. } |
            TraceEvent::SyncStart { id, ts, .. } |
            TraceEvent::ThreadStart { id, ts, .. } => {
                spans.entry(id).or_insert((None, None)).0 = Some(ts);
            }
            TraceEvent::AsyncOffCPU { id, ts, .. } |
            TraceEvent::AsyncEnd { id, ts, .. } |
            TraceEvent::SyncEnd { id, ts, .. } |
            TraceEvent::ThreadEnd { id, ts, .. } => {
                spans.entry(id).or_insert((None, None)).1 = Some(ts);
            }

            TraceEvent::Wakeup { .. } => {}
        }

        match event {
            TraceEvent::AsyncStart { parent_id, .. } |
            TraceEvent::SyncStart { parent_id, .. } => {
                parents.insert(parent_id);
            }

            _ => {}
        }
    }

    let mut spans = spans.into_iter().filter_map(|(k, v)| {
        if parents.contains(&k) {
            None
        } else {
            Some(((v.0).unwrap(), (v.1).unwrap()))
        }
    }).collect::<Vec<_>>();

    spans.sort();

    let min_time = spans[0].0;
    let max_time = spans.iter().map(|a| a.1).max().unwrap();

    let event_loop = glutin::event_loop::EventLoop::new();
    let wb = glutin::window::WindowBuilder::new();
    let cb = glutin::ContextBuilder::new().with_depth_buffer(24);
    let display = glium::Display::new(wb, cb, &event_loop).unwrap();

    #[derive(Copy, Clone)]
    struct Vertex {
        position: [f32; 2],
    }

    implement_vertex!(Vertex, position);

    let shape = glium::vertex::VertexBuffer::new(&display, &[
            Vertex { position: [-1.0,  1.0] },
            Vertex { position: [ 1.0,  1.0] },
            Vertex { position: [-1.0, -1.0] },
            Vertex { position: [ 1.0, -1.0] },
        ]).unwrap();

    let vertex_shader_src = r#"
        #version 150
        in vec2 position;
        uniform vec2 scale;
        uniform vec2 offset;
        void main() {
            gl_Position = vec4(position.xy*scale + offset, 0.0, 1.0);
        }
    "#;

    let fragment_shader_src = r#"
        #version 140
        out vec4 color;
        void main() {
            color = vec4(1.0, 0.0, 0.0, 1.0);
        }
    "#;

    let program = glium::Program::from_source(&display, vertex_shader_src, fragment_shader_src,
                                              None).unwrap();

    event_loop.run(move |event, _, control_flow| {
        let next_frame_time = std::time::Instant::now() +
            std::time::Duration::from_nanos(16_666_667);
        *control_flow = glutin::event_loop::ControlFlow::WaitUntil(next_frame_time);

        match event {
            glutin::event::Event::WindowEvent { event, .. } => match event {
                glutin::event::WindowEvent::CloseRequested => {
                    *control_flow = glutin::event_loop::ControlFlow::Exit;
                    return;
                },
                _ => return,
            },
            glutin::event::Event::NewEvents(cause) => match cause {
                glutin::event::StartCause::ResumeTimeReached { .. } => (),
                glutin::event::StartCause::Init => (),
                _ => return,
            },
            glutin::event::Event::MainEventsCleared | 
            glutin::event::Event::RedrawEventsCleared => return,
            _ => {
                // println!("{:?}", event);
                return;
            }
        }

        let mut target = display.draw();
        target.clear_color_and_depth((1.0, 1.0, 1.0, 1.0), 1.0);

        let scale: [f32; 2] = [0.5, 0.5];
        let offset: [f32; 2] = [0.0, 0.0];

        let params = glium::DrawParameters {
            depth: glium::Depth {
                test: glium::draw_parameters::DepthTest::IfLess,
                write: true,
                .. Default::default()
            },
            .. Default::default()
        };

        target.draw(&shape, glium::index::NoIndices(glium::index::PrimitiveType::TriangleStrip), &program,
                    &uniform! { scale: scale, offset: offset },
                    &params).unwrap();
        target.finish().unwrap();
    });
}
