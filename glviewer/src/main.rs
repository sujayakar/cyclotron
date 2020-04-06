use std::path::Path;
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
use std::collections::HashMap;
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

#[derive(Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
struct TraceSpan {
    // span is first so these are ordered in time
    span: Span,
    id: SpanId,
    parent: Option<SpanId>,
    kind: TraceKind,
    name: Option<String>,
    metadata: Option<String>,
}

enum WhichEnd {
    Begin,
    End,
}

#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
enum TraceKind {
    Sync,
    Async,
    AsyncCPU,
    // Thread,
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

#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
struct Span {
    begin: u64,
    end: u64,
}

#[derive(Copy, Clone)]
struct Vertex {
    position: [f32; 2],
    parent_ident: u32,
}

implement_vertex!(Vertex, position, parent_ident);

struct LaneBuilder {
    parent_ids: Vec<Option<SpanId>>,
    ids: Vec<SpanId>,
    spans: Vec<Span>,
}

impl LaneBuilder {
    fn new() -> LaneBuilder {
        LaneBuilder {
            parent_ids: Vec::new(),
            ids: Vec::new(),
            spans: Vec::new(),
        }
    }

    fn try_add(&mut self, parent_id: Option<SpanId>, id: SpanId, span: Span) -> Option<usize> {
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
                return None;
            }
        }

        if let Some(last) = self.spans.last() {
            assert!(last.begin <= span.begin);
        }
        let index = self.spans.len();
        self.spans.push(span);
        self.ids.push(id);
        self.parent_ids.push(parent_id);
        Some(index)
    }

    fn build(self, display: &Display) -> Lane {
        Lane::new(display, self.parent_ids, self.ids, self.spans)
    }
}

struct LaneAssignment {
    lane: usize,
    index: usize,
}

struct ViewBuilder {
    spans: HashMap<SpanId, TraceSpan>,
    assignments: HashMap<SpanId, LaneAssignment>,
    lanes: Vec<LaneBuilder>,
    min_time: u64,
    max_time: u64,
}

impl ViewBuilder {
    fn new() -> ViewBuilder {
        ViewBuilder {
            spans: HashMap::new(),
            assignments: HashMap::new(),
            lanes: Vec::new(),
            min_time: std::u64::MAX,
            max_time: 0,
        }
    }

    fn add(&mut self, span: TraceSpan) {
        self.spans.insert(span.id, span.clone());
        self.min_time = std::cmp::min(self.min_time, span.span.begin);
        self.max_time = std::cmp::max(self.max_time, span.span.end);

        let min_lane = if let Some(parent) = span.parent {
            if let Some(parent) = self.assignments.get(&parent) {
                parent.lane + 1
            } else {
                0
            }
        } else {
            0
        };

        for (lane_id, lane) in self.lanes.iter_mut().enumerate() {
            if lane_id > min_lane {
                if let Some(index) = lane.try_add(span.parent, span.id, span.span) {
                    self.assignments.insert(span.id, LaneAssignment { lane: lane_id, index });
                    return;
                }
            }
        }

        let mut lane = LaneBuilder::new();
        let index = lane.try_add(span.parent, span.id, span.span).unwrap();
        self.assignments.insert(span.id, LaneAssignment { lane: self.lanes.len(), index });
        self.lanes.push(lane);
    }

    fn build(self, display: &Display) -> View {
        let now = Instant::now();
        View {
            scale_offset: ScaleOffset::new(
                (self.min_time as f32) / 1_000_000_000.0,
                (self.max_time as f32) / 1_000_000_000.0),
            spans: self.spans,
            plain_rect: build_rect(display),
            lane_sizes: self.lanes.iter().map(|_| LaneSize {
                target: TargetSize::Full,
                old_size: 0.0,
                begin: now,
                end: now
            }).collect(),
            lanes: self.lanes.into_iter().map(|l| l.build(display)).collect(),
            highlight_lane: None,
        }
    }
}

fn build_rect(display: &Display) -> VertexBuffer<Vertex> {
    VertexBuffer::new(display, &[
        Vertex { position: [0.0, 0.0], parent_ident: 0xffffffff },
        Vertex { position: [1.0, 0.0], parent_ident: 0xffffffff },
        Vertex { position: [0.0, 1.0], parent_ident: 0xffffffff },
        Vertex { position: [1.0, 1.0], parent_ident: 0xffffffff },
    ]).unwrap()
}

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
enum TargetSize {
    Full,
    Hidden,
}

struct LaneSize {
    target: TargetSize,
    old_size: f32,
    begin: Instant,
    end: Instant,
}

struct View {
    scale_offset: ScaleOffset,
    spans: HashMap<SpanId, TraceSpan>,
    plain_rect: VertexBuffer<Vertex>,
    lanes: Vec<Lane>,
    lane_sizes: Vec<LaneSize>,
    highlight_lane: Option<(usize, Option<usize>)>,
}

struct LaneRect {
    offset: f32,
    scale: f32,
    min: f32,
    max: f32,
}

impl View {
    fn new(display: &Display, mut spans: Vec<TraceSpan>) -> View {
        spans.sort();
        let mut b = ViewBuilder::new();
        for span in spans {
            b.add(span);
        }
        b.build(display)
    }

    fn compute_lane_rects(&self) -> Vec<LaneRect> {
        let now = Instant::now();

        let actual_sizes = self.lane_sizes.iter().map(|size| {
            if now < size.begin {
                match size.target {
                    TargetSize::Full => 0.0,
                    TargetSize::Hidden => 1.0,
                }
            } else if now > size.end {
                match size.target {
                    TargetSize::Full => 1.0,
                    TargetSize::Hidden => 0.0,
                }
            } else {
                match size.target {
                    TargetSize::Full => 1.0,
                    TargetSize::Hidden => 0.0,
                }

                // XXX: this animation was making me sick.
                // let frac = (size.end - size.begin).as_secs_f32() / (now - size.begin).as_secs_f32();
                // match size.target {
                //     TargetSize::Full => frac,
                //     TargetSize::Hidden => 1.0 - frac,
                // }
            }
        }).collect::<Vec<_>>();

        let mut cum_sums = Vec::new();

        let mut total_size = 0.0;
        for size in &actual_sizes {
            cum_sums.push(total_size);
            total_size += size;
        }

        let mut res = Vec::new();

        for (size, cum_sum) in actual_sizes.iter().zip(cum_sums.iter()) {
            res.push(LaneRect {
                offset: *cum_sum,
                scale: size / total_size,
                min: cum_sum / total_size,
                max: (cum_sum + size) / total_size,
            });
        }

        res
    }

    fn hover(&mut self, pixel_coord: (i32, i32), display: (i32, i32)) {
        let scale = self.scale_offset.scale();
        let offset = self.scale_offset.offset();

        let old_highlight = self.highlight_lane;

        self.highlight_lane = None;

        let rects = self.compute_lane_rects();

        for (i, lane) in self.lanes.iter_mut().enumerate() {
            let pixel_begin = rects[i].min * display.1 as f32;
            let pixel_end = rects[i].max * display.1 as f32;

            if pixel_coord.1 as f32 >= pixel_begin && (pixel_coord.1 as f32) < pixel_end {
                let index = lane.hover(pixel_coord.0, display.0, scale, offset);
                self.highlight_lane = Some((i, index));
                if self.highlight_lane != old_highlight {
                    if let Some(index) = index {
                        let span_id = lane.ids[index];
                        let span = &self.spans[&span_id];
                        println!("{:?} {} {}",
                            span.kind,
                            span.name.as_ref().map(|s| s.as_str()).unwrap_or(""),
                            span.metadata.as_ref().map(|s| s.as_str()).unwrap_or(""));

                        if let Some(mut parent) = span.parent {
                            while let Some(span) = self.spans.get(&parent) {
                                println!("  parent {:?} {} {}",
                                    span.kind,
                                    span.name.as_ref().map(|s| s.as_str()).unwrap_or(""),
                                    span.metadata.as_ref().map(|s| s.as_str()).unwrap_or(""));
                                if let Some(p) = span.parent {
                                    parent = p;
                                } else {
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn draw(&self, program: &glium::Program, target: &mut Frame) {
        let scale = self.scale_offset.scale();
        let offset = self.scale_offset.offset();

        let params = glium::DrawParameters {
            depth: glium::Depth {
                test: DepthTest::Overwrite,
                write: true,
                .. Default::default()
            },
            .. Default::default()
        };

        let rects = self.compute_lane_rects();

        let highlight_item = if let Some((lane_id, Some(index))) = self.highlight_lane {
            (self.lanes[lane_id].ids[index].0 & 0xffffffff) as u32
        } else {
            0xfffffffe
        };

        for (i, lane) in self.lanes.iter().enumerate() {
            let color = hsl_to_rgb(i as f32 / self.lanes.len() as f32, 0.5, 0.2);

            let vert_offset = rects[i].offset;
            let vert_scale = rects[i].scale;

            if let Some((lane_id, _)) = self.highlight_lane {
                if i == lane_id {
                    let offset_vec: [f32; 2] = [0.0, vert_offset];
                    let scale_vec: [f32; 2] = [1.0, vert_scale];

                    target.draw(&self.plain_rect, glium::index::NoIndices(PrimitiveType::TriangleStrip), &program,
                        &uniform! { scale: scale_vec, offset: offset_vec, item_color: [0.9f32, 0.9, 0.9, 1.0] },
                        &params).unwrap();
                }
            }

            let offset_vec: [f32; 2] = [offset, vert_offset];
            let scale_vec: [f32; 2] = [scale, vert_scale];

            target.draw(&lane.vertex, &lane.index, &program,
                        &uniform! {
                            scale: scale_vec,
                            offset: offset_vec,
                            item_color: [color.0, color.1, color.2, 1.0],
                            parent_color: [1.0f32, 0.0, 1.0, 1.0],
                            highlight_item: highlight_item,
                        },
                        &params).unwrap();

            if let Some((lane_id, Some(index))) = self.highlight_lane {
                if i == lane_id {
                    let selection_index_buf = lane.index.slice(6*index .. 6*(index + 1)).unwrap();
                    target.draw(&lane.vertex, &selection_index_buf, &program,
                                &uniform! {
                                    scale: scale_vec,
                                    offset: offset_vec,
                                    item_color: [1.0f32, 0.0, 0.0, 1.0],
                                    parent_color: [1.0f32, 0.0, 0.0, 1.0],
                                    highlight_item: 0xfffffffeu32,
                                },
                                &params).unwrap();
                }
            }
        }
    }

    fn scroll(&mut self, offset_scroll: f32, scale_scroll: f32, ptr_loc: f32) {
        self.scale_offset.scroll(offset_scroll, scale_scroll, ptr_loc);
        
        let scale = self.scale_offset.scale();
        let offset = self.scale_offset.offset();

        // 0 = (left + offset) * scale
        let left = - offset;

        // 1 = (right + offset) * scale
        let right = 1.0/scale - offset;

        let left = (left * 1_000_000_000.0) as u64;
        let right = (right * 1_000_000_000.0) as u64;

        let now = Instant::now();

        let mut i = 0;
        for (lane, size) in self.lanes.iter().zip(self.lane_sizes.iter_mut()) {
            i += 1;
            let target = if lane.has_contents(left, right) {
                TargetSize::Full
            } else {
                TargetSize::Hidden
            };

            if target != size.target {
                size.target = target;
                size.begin = now;
                size.end = now + Duration::from_millis(1000);
            }
        }
    }
}

struct Lane {
    ids: Vec<SpanId>,
    spans: Vec<Span>,
    vertex: VertexBuffer<Vertex>,
    index: IndexBuffer<u32>,
}

impl Lane {
    fn new(display: &Display, parent_ids: Vec<Option<SpanId>>, ids: Vec<SpanId>, spans: Vec<Span>) -> Lane {
        let mut verts = Vec::new();
        let mut tris = Vec::<u32>::new();

        for (parent_id, span) in parent_ids.iter().zip(spans.iter()) {
            let parent_ident = parent_id.map(|p| (p.0  & 0xffffffff) as u32).unwrap_or(0xffffffff);

            let s = verts.len() as u32;
            tris.extend(&[s, s+1, s+2, s+1, s+2, s+3]);
            verts.push(Vertex { position: [(span.begin as f32) / 1_000_000_000.0, 0.0], parent_ident });
            verts.push(Vertex { position: [(span.end as f32) / 1_000_000_000.0, 0.0], parent_ident });
            verts.push(Vertex { position: [(span.begin as f32) / 1_000_000_000.0, 1.0], parent_ident });
            verts.push(Vertex { position: [(span.end as f32) / 1_000_000_000.0, 1.0], parent_ident });
        }

        let vertex = VertexBuffer::new(display, &verts).unwrap();
        let index = IndexBuffer::new(display, PrimitiveType::TrianglesList, &tris).unwrap();

        Lane {
            ids,
            spans,
            vertex,
            index,
        }
    }

    fn has_contents(&self, left: u64, right: u64) -> bool {
        let min = self.spans.binary_search_by_key(&left, |s| s.end).unwrap_or_else(|e| e);
        let max = self.spans.binary_search_by_key(&right, |s| s.begin).unwrap_or_else(|e| e);

        min < max
    }

    fn hover(&mut self, pixel_coord: i32, display: i32, scale: f32, offset: f32) -> Option<usize> {
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
                return Some(i);
            } else if begin > pixel_coord {
                break;
            }
        }

        None
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
    
    let (events, _wakeups) = read_events(&args.trace);

    let mut spans = HashMap::new();

    for event in events {
        let id = event.id;
        match event.end {
            WhichEnd::Begin => spans.entry(id).or_insert((None, None)).0 = Some(event),
            WhichEnd::End => spans.entry(id).or_insert((None, None)).1 = Some(event),
        }
    }

    let mut spans = spans.into_iter().map(|(_k, v)| {
        let begin = (v.0).unwrap();
        let end = (v.1).unwrap();
        
        let span = Span { begin: begin.nanos, end: end.nanos };

        TraceSpan {
            span,
            id: begin.id,
            parent: begin.parent,
            kind: begin.kind,
            name: begin.name,
            metadata: begin.metadata,
        }
    }).collect::<Vec<_>>();

    let min_time = (spans[0].span.begin as f32) / 1_000_000_000.0;
    let max_time = (spans.iter().map(|a| a.span.end).max().unwrap() as f32) / 1_000_000_000.0;

    let event_loop = glutin::event_loop::EventLoop::new();
    let wb = glutin::window::WindowBuilder::new()
        .with_title(format!("Cyclotron: {}", args.trace));
    let cb = glutin::ContextBuilder::new()
        .with_depth_buffer(24)
        .with_multisampling(8);
    let display = glium::Display::new(wb, cb, &event_loop).unwrap();

    let mut view = View::new(&display, spans);

    let vertex_shader_src = r#"
        #version 150
        in uint parent_ident;
        in vec2 position;

        uniform vec4 parent_color;
        uniform vec4 item_color;
        uniform vec2 scale;
        uniform vec2 offset;
        uniform uint highlight_item;
        
        out vec4 vert_color;
        
        void main() {
            vec2 pos0 = (position + offset)*scale;
            vec2 pos0_offset = pos0 - 0.5;
            gl_Position = vec4(2*pos0_offset.x, -2*pos0_offset.y, 0.0, 1.0);

            if(highlight_item == parent_ident) {
                vert_color = parent_color;
            } else {
                vert_color = item_color;
            }
        }
    "#;

    let fragment_shader_src = r#"
        #version 140
        in vec4 vert_color;
        out vec4 color;
        void main() {
            color = vert_color;
        }
    "#;

    let program = glium::Program::from_source(&display, vertex_shader_src, fragment_shader_src,
                                              None).unwrap();

    let mut scale_offset = ScaleOffset::new(min_time, max_time);

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
                    view.hover((position.x as i32, position.y as i32), (dims.0 as i32, dims.1 as i32));
                    ptr_loc = position.x as f32 / dims.0 as f32;
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
                    view.scroll(delta.x as f32, delta.y as f32, ptr_loc);
                }
                _ => {}
            },
            _ => {
                return;
            }
        }

        // frame_count += 1;

        let mut target = display.draw();
        target.clear_color_and_depth((1.0, 1.0, 1.0, 1.0), 1.0);

        view.draw(&program, &mut target);

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

fn read_events(path: impl AsRef<Path>) -> (Vec<TraceEvent>, Vec<TraceWakeup>) {

    let mut file = BufReader::new(File::open(path).unwrap());
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
                JsonTraceEvent::ThreadStart { id: _, ts: _, name: _ } => {}
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
                JsonTraceEvent::ThreadEnd { id: _, ts: _,  } => {}
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

    (events, wakeups)
}