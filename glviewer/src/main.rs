use glium::{
    glutin,
    Surface,
    Display,
    implement_vertex,
    uniform,
    index::{
        PrimitiveType,
        IndexBuffer
    },
    vertex::VertexBuffer,
    draw_parameters::DepthTest,
};
use structopt::StructOpt;
use cyclotron_backend::{
    TraceEvent as JsonTraceEvent,
    SpanId,
};
use std::io::{BufReader, BufRead};
use std::fs::File;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

struct TraceEvent {
    id: SpanId,
    end: WhichEnd,
    parent: Option<SpanId>,
    nanos: u64,
    kind: TraceKind,
    name: Option<String>,
    metadata: Option<String>,
}

enum WhichEnd {
    Begin,
    End,
}

enum TraceKind {
    Sync,
    Async,
    AsyncCPU,
    Thread,
}

struct TraceWakeup {
    waking_span: SpanId,
    parked_span: SpanId,
    nanos: u64,
}

#[derive(Debug, StructOpt)]
struct Args {
    trace: String,
    // grep: Vec<String>,
    // hide_wakeups: Vec<String>,
}

#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq)]
struct Span {
    begin: u64,
    end: u64,
}

#[derive(Copy, Clone)]
struct Vertex {
    position: [f32; 2],
}

implement_vertex!(Vertex, position);

struct LaneBuilder {

}

struct Lane {
    spans: Vec<Span>,
    vertex: VertexBuffer<Vertex>,
    index: IndexBuffer<u32>,
}

impl Lane {
    fn new(display: &Display, spans: Vec<Span>) -> Lane {
        let mut verts = Vec::new();
        let mut tris = Vec::<u32>::new();

        for span in &spans {
            let s = verts.len() as u32;
            tris.extend(&[s, s+1, s+2, s+1, s+2, s+3]);
            verts.push(Vertex { position: [(span.begin as f32) / 1_000_000_000.0, 0.0] });
            verts.push(Vertex { position: [(span.end as f32) / 1_000_000_000.0, 0.0] });
            verts.push(Vertex { position: [(span.begin as f32) / 1_000_000_000.0, 1.0] });
            verts.push(Vertex { position: [(span.end as f32) / 1_000_000_000.0, 1.0] });
        }

        let vertex = VertexBuffer::new(display, &verts).unwrap();
        let index = IndexBuffer::new(display, PrimitiveType::TrianglesList, &tris).unwrap();

        Lane {
            spans,
            vertex,
            index,
        }
    }
}

struct Scale {
    min_time: f32,
    max_time: f32,
    setting: f32,
}

impl Scale {
    fn eval(&self) -> f32 {
        let scale_base = 2.0 / (self.max_time - self.min_time);

        scale_base + self.setting.powf(2.0)*5000.0
    }

    fn scroll(&mut self, delta: f32) {
        self.setting += delta / 10000.0;
        if self.setting < 0.0 {
            self.setting = 0.0;
        }
        if self.setting > 1.0 {
            self.setting = 1.0;
        }
    }
}

fn main() {
    let args = Args::from_args();
    
    let mut file = BufReader::new(File::open(&args.trace).unwrap());
    let mut events = Vec::new();
    let mut wakeups = Vec::new();

    loop {
        let mut buf = String::new();
        let num_read = file.read_line(&mut buf).unwrap();

        if num_read == 0 || !buf.ends_with("\n") {
            break;
        } else {
            buf.pop();
            match serde_json::from_str(&buf).unwrap() {
                JsonTraceEvent::AsyncStart { id, ts, name, parent_id, metadata } => events.push(TraceEvent {
                    id,
                    end: WhichEnd::Begin,
                    kind: TraceKind::Async,
                    parent: Some(parent_id),
                    name: Some(name),
                    nanos: ts.as_nanos() as u64,
                    metadata: Some(serde_json::to_string(&metadata).unwrap()),
                }),
                JsonTraceEvent::AsyncOnCPU { id, ts } => events.push(TraceEvent {
                    id,
                    end: WhichEnd::Begin,
                    kind: TraceKind::AsyncCPU,
                    parent: None,
                    name: None,
                    nanos: ts.as_nanos() as u64,
                    metadata: None,
                }),
                JsonTraceEvent::SyncStart { id, ts, name, parent_id, metadata } => events.push(TraceEvent {
                    id,
                    end: WhichEnd::Begin,
                    kind: TraceKind::Sync,
                    parent: Some(parent_id),
                    name: Some(name),
                    nanos: ts.as_nanos() as u64,
                    metadata: Some(serde_json::to_string(&metadata).unwrap()),
                }),
                JsonTraceEvent::ThreadStart { id, ts, name } => events.push(TraceEvent {
                    id,
                    end: WhichEnd::Begin,
                    kind: TraceKind::Thread,
                    parent: None,
                    name: Some(name),
                    nanos: ts.as_nanos() as u64,
                    metadata: None,
                }),
                JsonTraceEvent::AsyncOffCPU { id, ts,  } => events.push(TraceEvent {
                    id,
                    end: WhichEnd::End,
                    kind: TraceKind::AsyncCPU,
                    parent: None,
                    name: None,
                    nanos: ts.as_nanos() as u64,
                    metadata: None,
                }),
                JsonTraceEvent::AsyncEnd { id, ts, outcome } => events.push(TraceEvent {
                    id,
                    end: WhichEnd::End,
                    kind: TraceKind::Async,
                    parent: None,
                    name: None,
                    nanos: ts.as_nanos() as u64,
                    metadata: Some(format!("{:?}", outcome)),
                }),
                JsonTraceEvent::SyncEnd { id, ts,  } => events.push(TraceEvent {
                    id,
                    end: WhichEnd::End,
                    kind: TraceKind::Sync,
                    parent: None,
                    name: None,
                    nanos: ts.as_nanos() as u64,
                    metadata: None,
                }),
                JsonTraceEvent::ThreadEnd { id, ts,  } => events.push(TraceEvent {
                    id,
                    end: WhichEnd::End,
                    kind: TraceKind::Thread,
                    parent: None,
                    name: None,
                    nanos: ts.as_nanos() as u64,
                    metadata: None,
                }),
                JsonTraceEvent::Wakeup { waking_span, parked_span, ts } => wakeups.push(TraceWakeup {
                    waking_span,
                    parked_span,
                    nanos: ts.as_nanos() as u64,
                })
            }
        }
    }

    let mut spans = HashMap::new();
    let mut parents = HashSet::new();

    for event in events {
        match event.end {
            WhichEnd::Begin => spans.entry(event.id).or_insert((None, None)).0 = Some(event.nanos),
            WhichEnd::End => spans.entry(event.id).or_insert((None, None)).1 = Some(event.nanos),
        }

        if let Some(parent_id) = event.parent {
            parents.insert(parent_id);
        }
    }

    let mut spans = spans.into_iter().filter_map(|(k, v)| {
        if parents.contains(&k) {
            None
        } else {
            Some(Span { begin: (v.0).unwrap(), end: (v.1).unwrap()})
        }
    }).collect::<Vec<_>>();

    spans.sort();

    let min_time = (spans[0].begin as f32) / 1_000_000_000.0;
    let max_time = (spans.iter().map(|a| a.end).max().unwrap() as f32) / 1_000_000_000.0;

    let event_loop = glutin::event_loop::EventLoop::new();
    let wb = glutin::window::WindowBuilder::new()
        .with_title(format!("Cyclotron: {}", args.trace));
    let cb = glutin::ContextBuilder::new()
        .with_depth_buffer(24)
        .with_multisampling(8);
    let display = glium::Display::new(wb, cb, &event_loop).unwrap();

    let lane = Lane::new(&display, spans.clone());

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
        uniform vec4 item_color;
        out vec4 color;
        void main() {
            color = item_color;
        }
    "#;

    let program = glium::Program::from_source(&display, vertex_shader_src, fragment_shader_src,
                                              None).unwrap();

    let mut scale = Scale {
        min_time,
        max_time,
        setting: 0.0,
    };
    let mut offset = -(max_time - min_time) / 2.0;

    let mut selection: Option<usize> = None;
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
                glutin::event::WindowEvent::CursorMoved { position, .. } => {
                    let dims = display.get_framebuffer_dimensions();
                    let logical_y = (position.y / dims.1 as f64) * 2.0 - 1.0;

                    let pixel_x = position.x as i32;

                    let scale = scale.eval();

                    let to_pixel_coord = |c: u64| {
                        let log = ((c as f32 / 1_000_000_000.0) + offset) * scale;
                        let pix = (log + 1.0) / 2.0 * (dims.0 as f32);
                        if pix >= 0.0 {
                            pix as i32
                        } else {
                            -10
                        }
                    };

                    selection = if logical_y >= -0.25 && logical_y <= 0.25 {
                        let min = match spans.binary_search_by_key(
                            &pixel_x, |s| to_pixel_coord(s.begin))
                        {
                            Ok(x) => x,
                            Err(x) => x.saturating_sub(1),
                        };

                        let mut selected_index = None;

                        for i in min..spans.len() {
                            let begin = to_pixel_coord(spans[i].begin);
                            let end = to_pixel_coord(spans[i].end);
                            if begin <= pixel_x && end >= pixel_x {
                                selected_index = Some(i);
                                break;
                            } else if begin > pixel_x {
                                break;
                            }
                        }

                        selected_index
                    } else {
                        None
                    };

                    println!("{:?}", selection);
                }
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

                    offset += delta.x as f32 / scale.eval() / 1000.0;

                    // left edge of screen:
                    // -1 == (min_time + offset)*scale
                    // -1/scale - min_time == offset

                    // right edge of screen:
                    // 1 == (max_time + offset)*scale
                    // 1/scale - max_time == offset

                    let a = -1.0 / scale.eval() - min_time;
                    let b = 1.0 / scale.eval() - max_time;
                    let (offset_min, offset_max) = if a < b { (a, b) } else { (b, a) };

                    if offset < offset_min {
                        offset = offset_min;
                    }

                    if offset > offset_max {
                        offset = offset_max;
                    }

                    scale.scroll(delta.y as f32);
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

        let offset_vec: [f32; 2] = [offset, -0.5];
        let render_scale = scale.eval();
        let scale_vec: [f32; 2] = [render_scale, 0.5];

        let params = glium::DrawParameters {
            depth: glium::Depth {
                test: DepthTest::Overwrite,
                write: true,
                .. Default::default()
            },
            .. Default::default()
        };

        target.draw(&lane.vertex, &lane.index, &program,
                    &uniform! { scale: scale_vec, offset: offset_vec, item_color: [0.0f32, 0.0, 0.0, 1.0] },
                    &params).unwrap();

        if let Some(selection) = selection {
            let selection_index_buf = lane.index.slice(6*selection .. 6*(selection + 1)).unwrap();
            target.draw(&lane.vertex, &selection_index_buf, &program,
                        &uniform! { scale: scale_vec, offset: offset_vec, item_color: [1.0f32, 0.0, 0.0, 1.0] },
                        &params).unwrap();
        }

        target.finish().unwrap();
    });
}
