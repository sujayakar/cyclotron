use glium::{
    glutin,
    Surface,
    Display,
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
    spans: Vec<Span>,
}

impl LaneBuilder {
    fn new() -> LaneBuilder {
        LaneBuilder {
            spans: Vec::new(),
        }
    }

    fn try_add(&mut self, span: Span) -> bool {
        let min = match self.spans.binary_search_by_key(&span.begin, |s| s.begin) {
            Ok(v) => v,
            Err(v) => v.saturating_sub(1),
        };

        for i in min..self.spans.len() {
            let s = self.spans[i];
            if s.begin >= span.end {
                break;
            }
            if s.end > span.begin {
                return false;
            }
        }

        if let Some(last) = self.spans.last() {
            assert!(last.begin <= span.begin);
        }
        self.spans.push(span);

        true
    }

    fn build(self, display: &Display) -> Lane {
        Lane::new(display, self.spans)
    }
}

struct ViewBuilder {
    lanes: Vec<LaneBuilder>,
}

impl ViewBuilder {
    fn new() -> ViewBuilder {
        ViewBuilder {
            lanes: Vec::new(),
        }
    }

    fn add(&mut self, span: Span) {
        for lane in self.lanes.iter_mut() {
            if lane.try_add(span) {
                return;
            }
        }

        let mut lane = LaneBuilder::new();
        assert!(lane.try_add(span));
        self.lanes.push(lane);
    }

    fn build(self, display: &Display) -> View {
        View {
            plain_rect: build_rect(display),
            lanes: self.lanes.into_iter().map(|l| l.build(display)).collect(),
            highlight_lane: None,
        }
    }
}

fn build_rect(display: &Display) -> VertexBuffer<Vertex> {
    VertexBuffer::new(display, &[
        Vertex { position: [0.0, 0.0] },
        Vertex { position: [1.0, 0.0] },
        Vertex { position: [0.0, 1.0] },
        Vertex { position: [1.0, 1.0] },
    ]).unwrap()
}

struct View {
    plain_rect: VertexBuffer<Vertex>,
    lanes: Vec<Lane>,
    highlight_lane: Option<usize>,
}

impl View {
    fn new(display: &Display, mut spans: Vec<Span>) -> View {
        spans.sort();
        let mut b = ViewBuilder::new();
        for span in spans {
            b.add(span);
        }
        b.build(display)
    }

    fn hover(&mut self, pixel_coord: (i32, i32), display: (i32, i32), scale_offset: &ScaleOffset) {
        let scale = scale_offset.scale();
        let offset = scale_offset.offset();

        self.highlight_lane = None;

        let lanes_len = self.lanes.len();
        for (i, lane) in self.lanes.iter_mut().enumerate() {
            let pixel_begin = (i + 1) as f32 / (lanes_len + 2) as f32 * display.1 as f32;
            let pixel_end = (i + 2) as f32 / (lanes_len + 2) as f32 * display.1 as f32;

            lane.selected.clear();
            if pixel_coord.1 as f32 >= pixel_begin && (pixel_coord.1 as f32) < pixel_end {
                self.highlight_lane = Some(i);
                lane.hover(pixel_coord.0, display.0, scale, offset);
            }
        }
    }

    fn draw(&self, program: &glium::Program, target: &mut Frame, scale_offset: &ScaleOffset) {
        let scale = scale_offset.scale();
        let offset = scale_offset.offset();

        let params = glium::DrawParameters {
            depth: glium::Depth {
                test: DepthTest::Overwrite,
                write: true,
                .. Default::default()
            },
            .. Default::default()
        };

        // let offset_vec: [f32; 2] = [0.0, 0.5];
        // let scale_vec: [f32; 2] = [1.0, 0.5];

        // target.draw(&self.plain_rect, glium::index::NoIndices(PrimitiveType::TriangleStrip), &program,
        //     &uniform! { scale: scale_vec, offset: offset_vec, item_color: [0.9f32, 0.9, 0.9, 1.0] },
        //     &params).unwrap();

        for (i, lane) in self.lanes.iter().enumerate() {
            let color = hsl_to_rgb(i as f32 / self.lanes.len() as f32, 0.5, 0.2);

            let vert_scale = 1.0 / (self.lanes.len() + 2) as f32;
            let vert_offset = (i + 1) as f32;

            if Some(i) == self.highlight_lane {
                let offset_vec: [f32; 2] = [0.0, vert_offset];
                let scale_vec: [f32; 2] = [1.0, vert_scale];

                target.draw(&self.plain_rect, glium::index::NoIndices(PrimitiveType::TriangleStrip), &program,
                    &uniform! { scale: scale_vec, offset: offset_vec, item_color: [0.9f32, 0.9, 0.9, 1.0] },
                    &params).unwrap();
            }

            let offset_vec: [f32; 2] = [offset, vert_offset];
            let scale_vec: [f32; 2] = [scale, vert_scale];

            target.draw(&lane.vertex, &lane.index, &program,
                        &uniform! { scale: scale_vec, offset: offset_vec, item_color: [color.0, color.1, color.2, 1.0] },
                        &params).unwrap();

            for (selected, color) in &lane.selected {
                let selection_index_buf = lane.index.slice(6*selected .. 6*(selected + 1)).unwrap();
                target.draw(&lane.vertex, &selection_index_buf, &program,
                            &uniform! { scale: scale_vec, offset: offset_vec, item_color: [color[0], color[1], color[2], 1.0] },
                            &params).unwrap();
            }
        }
    }
}

struct Lane {
    spans: Vec<Span>,
    vertex: VertexBuffer<Vertex>,
    index: IndexBuffer<u32>,
    selected: Vec<(usize, [f32; 4])>,
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
            selected: Vec::new(),
        }
    }

    fn hover(&mut self, pixel_coord: i32, display: i32, scale: f32, offset: f32) {
        let to_pixel_coord = |c: u64| {
            let pix = ((c as f32 / 1_000_000_000.0) + offset) * scale * display as f32;
            if pix >= 0.0 {
                pix as i32
            } else if pix > display as f32 {
                display + 10
            } else {
                -10
            }
        };

        let min = match self.spans.binary_search_by_key(
            &pixel_coord, |s| to_pixel_coord(s.begin))
        {
            Ok(x) => x,
            Err(x) => x.saturating_sub(1),
        };

        for i in min..self.spans.len() {
            let begin = to_pixel_coord(self.spans[i].begin);
            let end = to_pixel_coord(self.spans[i].end);
            if begin <= pixel_coord && end >= pixel_coord {
                self.selected.push((i, [1.0, 0.0, 0.0, 1.0]));
                break;
            } else if begin > pixel_coord {
                break;
            }
        }
    }
}

struct ScaleOffset {
    offset: f32,
    min_time: f32,
    max_time: f32,
    scale_setting: f32,
}

impl ScaleOffset {
    fn new(min_time: f32, max_time: f32) -> ScaleOffset {
        ScaleOffset {
            offset: -min_time,
            min_time,
            max_time,
            scale_setting: 0.0,
        }
    }

    fn offset(&self) -> f32 {
        self.offset
    }

    fn scale(&self) -> f32 {
        let scale_base = 1.0 / (self.max_time - self.min_time);

        scale_base + self.scale_setting.powf(2.0)*5000.0
    }

    fn scroll(&mut self, offset_scroll: f32, scale_scroll: f32, ptr_loc: f32) {
        // original_pos = (origin + offset0) * scale0
        // final_pos = (origin + offset0 + fixup) * scale1

        // (origin + offset0) * scale0 = (origin + offset0 + fixup) * scale1
        // (origin + offset0) * (scale0 / scale1 - 1) = fixup

        let scale_orig = self.scale();
        let origin = ptr_loc / scale_orig - self.offset;

        self.scale_setting += scale_scroll / 10000.0;
        if self.scale_setting < 0.0 {
            self.scale_setting = 0.0;
        }
        if self.scale_setting > 1.0 {
            self.scale_setting = 1.0;
        }

        let scale = self.scale();

        let fixup = (origin + self.offset) * (scale_orig / scale - 1.0);

        let offset_delta = offset_scroll / scale / 1000.0;

        self.offset += offset_delta + fixup;

        // left edge of screen:
        // 0 == (min_time + offset)*scale
        // - min_time == offset

        // right edge of screen:
        // 1 == (max_time + offset)*scale
        // 1/scale - max_time == offset

        let a = -self.min_time;
        let b = 1.0 / scale - self.max_time;
        let (offset_min, offset_max) = if a < b { (a, b) } else { (b, a) };

        if self.offset < offset_min {
            self.offset = offset_min;
        }

        if self.offset > offset_max {
            self.offset = offset_max;
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
                JsonTraceEvent::ThreadStart { id, ts, name } => {}
                // JsonTraceEvent::ThreadStart { id, ts, name } => events.push(TraceEvent {
                //     id,
                //     end: WhichEnd::Begin,
                //     kind: TraceKind::Thread,
                //     parent: None,
                //     name: Some(name),
                //     nanos: ts.as_nanos() as u64,
                //     metadata: None,
                // }),
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
                JsonTraceEvent::ThreadEnd { id, ts,  } => {}
                // JsonTraceEvent::ThreadEnd { id, ts,  } => events.push(TraceEvent {
                //     id,
                //     end: WhichEnd::End,
                //     kind: TraceKind::Thread,
                //     parent: None,
                //     name: None,
                //     nanos: ts.as_nanos() as u64,
                //     metadata: None,
                // }),
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

    let mut spans = spans.into_iter().map(|(_k, v)| {
        Span { begin: (v.0).unwrap(), end: (v.1).unwrap()}
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

    let mut view = View::new(&display, spans.clone());

    let vertex_shader_src = r#"
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

    let mut scale_offset = ScaleOffset::new(min_time, max_time);

    let mut selection: Option<usize> = None;
    let mut frame_count = 0;
    let begin = Instant::now();
    let mut ptr_loc = 0.0;

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
                    println!("{:?}, {:?}", position, dims);
                    let logical_y = (position.y / dims.1 as f64) * 2.0 - 1.0;

                    let pixel_x = position.x as i32;

                    view.hover((position.x as i32, position.y as i32), (dims.0 as i32, dims.1 as i32), &scale_offset);

                    let offset = scale_offset.offset();
                    let scale = scale_offset.scale();

                    ptr_loc = (position.x as f32 / dims.0 as f32);

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

                    // println!("{:?}", selection);
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
                    scale_offset.scroll(delta.x as f32, delta.y as f32, ptr_loc);
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

        view.draw(&program, &mut target, &scale_offset);

        // target.draw(&lane.vertex, &lane.index, &program,
        //             &uniform! { scale: scale_vec, offset: offset_vec, item_color: [0.0f32, 0.0, 0.0, 1.0] },
        //             &params).unwrap();

        // if let Some(selection) = selection {
        //     let selection_index_buf = lane.index.slice(6*selection .. 6*(selection + 1)).unwrap();
        //     target.draw(&lane.vertex, &selection_index_buf, &program,
        //                 &uniform! { scale: scale_vec, offset: offset_vec, item_color: [1.0f32, 0.0, 0.0, 1.0] },
        //                 &params).unwrap();
        // }

        target.finish().unwrap();
    });
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