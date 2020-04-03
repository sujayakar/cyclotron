use glium::{glutin, Surface, implement_vertex, uniform, index::PrimitiveType};
use structopt::StructOpt;
use cyclotron_backend::TraceEvent;
use std::io::{BufReader, BufRead};
use std::fs::File;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

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

    let min_time = spans[0].0.as_secs_f32();
    let max_time = spans.iter().map(|a| a.1).max().unwrap().as_secs_f32();

    let event_loop = glutin::event_loop::EventLoop::new();
    let wb = glutin::window::WindowBuilder::new()
        .with_title(format!("Cyclotron: {}", args.trace));
    let cb = glutin::ContextBuilder::new()
        .with_depth_buffer(24)
        .with_multisampling(8);
    let display = glium::Display::new(wb, cb, &event_loop).unwrap();

    #[derive(Copy, Clone)]
    struct Vertex {
        position: [f32; 2],
    }

    implement_vertex!(Vertex, position);

    // let vertex_buf = glium::vertex::VertexBuffer::new(&display, &[
    //         Vertex { position: [-1.0,  1.0] },
    //         Vertex { position: [ 1.0,  1.0] },
    //         Vertex { position: [-1.0, -1.0] },
    //         Vertex { position: [ 1.0, -1.0] },
    //     ]).unwrap();

    // let index_buf = glium::index::IndexBuffer::new(&display, PrimitiveType::TrianglesList, &[0u32, 1, 2, 1, 2, 3]).unwrap();


    let mut verts = Vec::new();
    let mut tris = Vec::<u32>::new();
    for (a, b) in spans {
        let s = verts.len() as u32;
        tris.extend(&[s, s+1, s+2, s+1, s+2, s+3]);
        verts.push(Vertex { position: [a.as_secs_f32(), 0.0] });
        verts.push(Vertex { position: [b.as_secs_f32(), 0.0] });
        verts.push(Vertex { position: [a.as_secs_f32(), 1.0] });
        verts.push(Vertex { position: [b.as_secs_f32(), 1.0] });
    }

    let vertex_buf = glium::vertex::VertexBuffer::new(&display, &verts).unwrap();
    let index_buf = glium::index::IndexBuffer::new(&display, PrimitiveType::TrianglesList, &tris).unwrap();

    let vertex_shader_src = r#"
        #version 150
        in vec2 position;
        uniform vec2 scale;
        uniform vec2 offset;
        void main() {
            gl_Position = vec4((position.xy + offset)*scale, 0.0, 1.0);
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

    let mut scale: [f32; 2] = [2.0 / (max_time - min_time), 0.5];
    let mut offset: [f32; 2] = [-(max_time - min_time) / 2.0, -0.5];

    let mut frame_count = 0;
    let begin = Instant::now();

    event_loop.run(move |event, _, control_flow| {
        let next_frame_time = Instant::now() + Duration::from_nanos(16_666_667);
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
            glutin::event::Event::DeviceEvent { event, .. } => match event {
                glutin::event::DeviceEvent::MouseWheel { delta: 
                    glutin::event::MouseScrollDelta::PixelDelta(delta) } => {

                    let dims = display.get_framebuffer_dimensions();

                    offset[0] += delta.x as f32 / dims.0 as f32 / scale[0] * 2.0;

                    if offset[0] < -max_time {
                        offset[0] = -max_time;
                    }

                    if offset[0] > 0.0 {
                        offset[0] = 0.0
                    }

                    scale[0] += delta.y as f32 / dims.1 as f32 * 10.0;

                    if scale[0] < 2.0 / (max_time - min_time) {
                        scale[0] = 2.0 / (max_time - min_time);
                    }

                    println!("{:?}", delta);
                }
                _ => {}
            },
            _ => {
                // println!("{:?}", event);
                return;
            }
        }

        // frame_count += 1;
        // println!("fps {}", frame_count as f32 / begin.elapsed().as_secs_f32());

        let mut target = display.draw();
        target.clear_color_and_depth((1.0, 1.0, 1.0, 1.0), 1.0);

        let params = glium::DrawParameters {
            depth: glium::Depth {
                test: glium::draw_parameters::DepthTest::IfLess,
                write: true,
                .. Default::default()
            },
            .. Default::default()
        };

        target.draw(&vertex_buf, &index_buf, &program,
                    &uniform! { scale: scale, offset: offset },
                    &params).unwrap();
        target.finish().unwrap();
    });
}
