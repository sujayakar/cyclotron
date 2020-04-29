use std::collections::HashMap;
use crate::db::{Span, NameId, TaskId};
use crate::layout::{Layout, ThreadId, RowId, BoxListKey, SpanRange, LabelListKey};
use crate::render::{DrawCommand, Color, Region, SimpleRegion};
use crate::util::hsl_to_rgb;

pub struct View {
    cursor: (f64, f64),
    cursor_down: Option<(f64, f64)>,
    mode: Mode,
    derived: Derived,
    limits: Span,
    span: Span,
}

fn bounded(a: u64, b: u64, c: u64) -> u64 {
    if b < a {
        a
    } else if b > c {
        c
    } else {
        b
    }
}

fn maxf(a: f64, b: f64) -> f64 {
    if a > b {
        a
    } else {
        b
    }
}

fn lerp(a: f64, b: f64, factor: f64) -> f64 {
    a * (1.0 - factor) + b * factor
}

fn minmaxf(a: f64, b: f64) -> (f64, f64) {
    if a > b {
        (b, a)
    } else {
        (a, b)
    }
}

const MIN_WIDTH: f64 = 1e5;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Mode {
    Trace,
    Profile,
}

#[derive(Eq, PartialEq, Copy, Clone)]
pub enum SelectionInfo {
    Span {
        task: TaskId,
        name: NameId,
        span: Span,
    },
    ProfileName {
        name: NameId,
        time: u64,
    }
}

#[derive(Copy, Clone)]
struct InternalSelectionInfo {
    key: BoxListKey,
    task: TaskId,
    name: NameId,
    index: usize,
    span: Span,
}

struct InternalProfileSelectionInfo {
    name: NameId,
    time: u64,
    thread_base: f32,
    thread_limit: f32,
    base: f32,
    limit: f32,
}

impl View {
    pub fn new(layout: &Layout) -> View {
        let limits = layout.span_discounting_threads();
        let cursor = (0.0, 0.0);
        let mode = Mode::Trace;
        View {
            cursor,
            mode,
            cursor_down: None,
            derived: derived(cursor, limits, mode, layout),
            limits,
            span: limits,
        }
    }

    pub fn toggle_mode(&mut self, layout: &Layout) {
        self.mode = match self.mode {
            Mode::Trace => Mode::Profile,
            Mode::Profile => Mode::Trace,
        };
        self.derived = derived(self.cursor, self.span, self.mode, layout);
    }

    pub fn relayout(&mut self, layout: &Layout) {
        self.limits = layout.span_discounting_threads();
        self.set_span(layout, self.span);
    }

    pub fn begin_drag(&mut self) {
        self.cursor_down = Some(self.cursor);
    }

    pub fn cancel_drag(&mut self) {
        self.cursor_down = None;
    }

    // Returns the old span
    pub fn end_drag(&mut self) -> Span {
        if let Some(cursor_down) = self.cursor_down {
            let old = self.span;

            let (left, right) = minmaxf(cursor_down.0, self.cursor.0);

            let begin = (self.span.begin as f64) * (1.0 - left) + (self.span.end as f64) * left;
            let end = (self.span.begin as f64) * (1.0 - right) + (self.span.end as f64) * right;

            self.span.begin = bounded(self.span.begin, begin as u64, self.limits.end - MIN_WIDTH as u64);
            self.span.end = bounded(self.span.begin + MIN_WIDTH as u64, end as u64, self.limits.end);

            self.cursor_down = None;
            old
        } else {
            panic!("end drag without start?");
        }
    }

    pub fn selection(&self) -> Option<SelectionInfo> {
        match &self.derived.mode {
            DerivedMode::Trace { selection: Some(selection), .. } => {
                Some(SelectionInfo::Span {
                    name: selection.name,
                    span: selection.span,
                    task: selection.task,
                })
            }
            DerivedMode::Profile { selection: Some(selection), .. } => {
                Some(SelectionInfo::ProfileName {
                    name: selection.name,
                    time: selection.time
                })
            }
            _ => None
        }
    }

    pub fn hover(&mut self, layout: &Layout, coord: (f64, f64)) {
        self.cursor = coord;
        self.derived.hover(self.cursor, self.span, layout);
    }

    pub fn cursor_time(&self) -> u64 {
        ((self.span.begin as f64) * (1.0 - self.cursor.0) + (self.span.end as f64) * self.cursor.0) as u64
    }

    pub fn span_time(&self) -> u64 {
        self.span.end - self.span.begin
    }

    pub fn set_span(&mut self, layout: &Layout, span: Span) {
        self.span.begin = bounded(self.limits.begin, span.begin, self.limits.end - MIN_WIDTH as u64);
        self.span.end = bounded(self.span.begin + MIN_WIDTH as u64, span.end, self.limits.end);

        self.derived = derived(self.cursor, self.span, self.mode, layout);
    }

    pub fn set_span_full(&mut self, layout: &Layout) {
        self.set_span(layout, self.limits);
    }

    pub fn scroll(&mut self, layout: &Layout, offset: f64, scale: f64) {
        if self.mode == Mode::Profile {
            return;
        }

        let factor = 1.05f64.powf(-scale / 1e1);

        let cursor = self.cursor.0;

        let begin = self.span.begin as f64;
        let end = self.span.end as f64;

        let x_delta = offset * (end - begin) / 1000.0;

        let orig_new_width = (end - begin) * factor;
        let mut new_width = orig_new_width;

        let max_width = (self.limits.end - self.limits.begin) as f64;

        if new_width < MIN_WIDTH {
            new_width = MIN_WIDTH;
        }

        if new_width > max_width {
            new_width = max_width;
        }

        let new_begin = lerp(begin + x_delta, end - new_width + x_delta, cursor);
        let new_end = new_begin + new_width;

        self.span.begin = bounded(self.limits.begin, maxf(0.0, new_begin) as u64, self.limits.end - MIN_WIDTH as u64);
        self.span.end = bounded(self.span.begin + MIN_WIDTH as u64, new_end as u64, self.limits.end);
        self.derived = derived(self.cursor, self.span, self.mode, layout);
    }

    pub fn draw_commands(&self) -> Vec<DrawCommand> {
        let mut res = Vec::new();

        let (r, g, b) = hsl_to_rgb(0., 0.68, 0.35);
        let primary_selection = Color { r, g, b, a: 1.0 };

        let (r, g, b) = hsl_to_rgb(0.67, 0.90, 0.35);
        let secondary_selection = Color { r, g, b, a: 1.0 };

        match &self.derived.mode {
            DerivedMode::Trace { rows, selection } => {
                if let Some(total) = rows.last().map(|r| r.limit) {
                    let (name, highlight) = if let Some(selection) = selection {
                        (Some(selection.name), secondary_selection)
                    } else {
                        (None, secondary_selection)
                    };

                    for row in rows {
                        let region = Region {
                            logical_base: (self.span.begin as f32) / 1e9,
                            logical_limit: (self.span.end as f32) / 1e9,

                            vertical_base: row.base / total,
                            vertical_limit: row.limit / total,
                        };

                        for subrow in &row.subrows {
                            res.push(DrawCommand::BoxList {
                                key: subrow.key,
                                color: subrow.color,
                                range: subrow.range,
                                name,
                                highlight,
                                region,
                            });

                            if let Some(selection) = selection {
                                if selection.key == subrow.key {
                                    res.push(DrawCommand::BoxList {
                                        key: selection.key,
                                        color: primary_selection,
                                        range: SpanRange { begin: selection.index, end: selection.index + 1 },
                                        name: None,
                                        highlight: primary_selection,
                                        region,
                                    })
                                }
                            }
                        }

                        res.push(DrawCommand::LabelList {
                            key: LabelListKey(row.thread_id, row.row_id),
                            region,
                        })
                    }
                }

                if let Some(cursor_down) = self.cursor_down {
                    let (left, right) = minmaxf(cursor_down.0, self.cursor.0);
                    // let (bottom, top) = minmaxf(cursor_down.1, self.cursor.1);
                    res.push(DrawCommand::SimpleBox {
                        color: Color { r: 0.0, g: 0.0, b: 0.0, a: 0.4 },
                        region: SimpleRegion {
                            left: left as f32,
                            right: right as f32,
                            // top: top as f32,
                            // bottom: bottom as f32,
                            bottom: 0.0,
                            top: 1.0,
                        },
                    })
                }
            }
            DerivedMode::Profile { threads, selection } => {
                let total_time = (self.span.end - self.span.begin) as f32;

                if let Some(total_height) = threads.last().and_then(|t| t.rows.last().map(|r| r.limit)) {

                    if let Some(selection) = selection {
                        res.push(DrawCommand::SimpleBox {
                            color: Color { r: 0.0, g: 1.0, b: 1.0, a: 0.4 },
                            region: SimpleRegion {
                                left: 0.0,
                                right: 1.0,
                                bottom: selection.thread_base / total_height,
                                top: selection.thread_limit / total_height,
                            },
                        });
                        res.push(DrawCommand::SimpleBox {
                            color: Color { r: 0.0, g: 0.0, b: 1.0, a: 1.0 },
                            region: SimpleRegion {
                                left: 0.0,
                                right: 1.0,
                                bottom: selection.base / total_height,
                                top: selection.limit / total_height,
                            },
                        });
                    }

                    for thread in threads {
                        for row in &thread.rows {
                            res.push(DrawCommand::SimpleBox {
                                color: Color { r: 0.0, g: 0.0, b: 0.0, a: 0.4 },
                                region: SimpleRegion {
                                    left: 0.0,
                                    right: row.time as f32 / total_time,
                                    bottom: row.base / total_height,
                                    top: row.limit / total_height,
                                },
                            })
                        }
                    }
                }
            }
        }

        res
    }
}

fn rows(span: Span, layout: &Layout) -> Vec<Row> {
    let mut res = Vec::new();
    let mut base = 0.0;

    for (tid, t) in layout.threads.iter().enumerate() {
        for (rid, r) in t.rows.iter().enumerate() {
            let mut subrows = Vec::new();

            for (ch, val, alpha) in &[(&r.back, true, 0.), (&r.fore, false, 0.5)] {
                if ch.has_overlap(span) {
                    subrows.push(Subrow {
                        key: BoxListKey(ThreadId(tid), RowId(rid), *val),
                        color: Color { r: 0.0, g: 0.0, b: 0.0, a: *alpha },
                        // TODO: sub-select a range
                        range: SpanRange { begin: 0, end: ch.begins.len() },
                    });
                }
            }

            if subrows.len() > 0 {
                res.push(Row {
                    thread_id: ThreadId(tid),
                    row_id: RowId(rid),
                    subrows,
                    base,
                    limit: base + 1.0,
                });
                base += 1.0;
            }
        }
    }

    res
}

fn profile_rows(span: Span, layout: &Layout) -> Vec<ProfileThread> {
    let mut res = Vec::new();
    let mut base = 0.0;

    for (_tid, t) in layout.threads.iter().enumerate() {
        let mut cpu_time_per_name = HashMap::new();

        for (_rid, r) in t.rows.iter().enumerate() {
            for (name, (begin, end)) in r.fore.names.iter().zip(r.fore.begins.iter().zip(&r.fore.ends)) {
                let begin = std::cmp::max(*begin, span.begin);
                let end = std::cmp::max(begin, std::cmp::min(*end, span.end));

                if end - begin > 0 {
                    *cpu_time_per_name.entry(name).or_insert(0) += end - begin;
                }
            }
        }

        let mut cpu_time_per_name: Vec<_> = cpu_time_per_name.into_iter().collect();
        cpu_time_per_name.sort_by(|a, b| (b.1).cmp(&a.1));

        let mut rows = Vec::new();

        for (name, time) in cpu_time_per_name {
            rows.push(ProfileRow {
                time,
                name: *name,
                color: Color { r: 1.0, g: 0.0, b: 0.0, a: 1.0 },
                base,
                limit: base + 1.0,
            });
            base += 1.0;
        }

        res.push(ProfileThread {
            rows,
        })
    }

    res
}

fn find_selection(cursor: (f64, f64), span: Span, rows: &[Row], layout: &Layout) -> Option<InternalSelectionInfo> {
    let x_value = (cursor.0 * (span.end - span.begin) as f64) as u64 + span.begin;

    if let Some(total) = rows.last().map(|r| r.limit) {
        for row in rows.iter() {
            let vertical_base = row.base / total;
            let vertical_limit = row.limit / total;
            if cursor.1 < vertical_base as f64 || cursor.1 >= vertical_limit as f64 {
                continue;
            }

            for subrow in row.subrows.iter().rev() {
                let key = subrow.key;
                let row_data = &layout.threads[(key.0).0].rows[(key.1).0];
                let chunk = if key.2 {
                    &row_data.back
                } else {
                    &row_data.fore
                };

                if let Some(index) = chunk.find(x_value) {
                    return Some(InternalSelectionInfo {
                        key,
                        task: chunk.tasks[index],
                        name: chunk.names[index],
                        index,
                        span: Span { begin: chunk.begins[index], end: chunk.ends[index] }
                    });
                }
            }
        }
    }
    None
}

fn find_profile_selection(cursor: (f64, f64), span: Span, threads: &[ProfileThread], layout: &Layout) -> Option<InternalProfileSelectionInfo> {
    if let Some(total_height) = threads.last().and_then(|t| t.rows.last().map(|r| r.limit)) {
        for thread in threads {
            for row in &thread.rows {
                let base = row.base as f64 / total_height as f64;
                let limit = row.limit as f64 / total_height as f64;
                if cursor.1 >= base && cursor.1 <= limit {
                    let thread_base = thread.rows[0].base;
                    let thread_limit = thread.rows.last().unwrap().limit;
                    return Some(InternalProfileSelectionInfo {
                        name: row.name,
                        time: row.time,
                        thread_base,
                        thread_limit,
                        base: row.base,
                        limit: row.limit,
                    })
                }
            }
        }
    }
    None
}

fn derived(cursor: (f64, f64), span: Span, mode: Mode, layout: &Layout) -> Derived {
    match mode {
        Mode::Trace => {
            let rows = rows(span, layout);

            let selection = find_selection(cursor, span, &rows, layout);

            Derived {
                mode: DerivedMode::Trace {
                    rows,
                    selection,
                },
            }
        }
        Mode::Profile => {
            let threads = profile_rows(span, layout);

            let selection = find_profile_selection(cursor, span, &threads, layout);

            Derived {
                mode: DerivedMode::Profile {
                    threads,
                    selection,
                },
            }
        }
    }
}

impl Derived {
    fn hover(&mut self, cursor: (f64, f64), span: Span, layout: &Layout) {
        match self.mode {
            DerivedMode::Trace { ref rows, ref mut selection } => {
                *selection = find_selection(cursor, span, rows, layout);
            }
            DerivedMode::Profile { ref threads, ref mut selection } => {
                *selection = find_profile_selection(cursor, span, threads, layout)
            }
        }
    }
}

struct Derived {
    mode: DerivedMode,
}

enum DerivedMode {
    Trace {
        rows: Vec<Row>,
        selection: Option<InternalSelectionInfo>,
    },
    Profile {
        threads: Vec<ProfileThread>,
        selection: Option<InternalProfileSelectionInfo>,
    },
}

struct Subrow {
    key: BoxListKey,
    range: SpanRange,
    color: Color,
}

struct Row {
    thread_id: ThreadId,
    row_id: RowId,
    subrows: Vec<Subrow>,
    base: f32,
    limit: f32,
}

struct ProfileThread {
    rows: Vec<ProfileRow>,
}

struct ProfileRow {
    time: u64,
    color: Color,
    name: NameId,
    base: f32,
    limit: f32,
}
