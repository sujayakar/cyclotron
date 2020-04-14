use crate::db::{Span, NameId, TaskId};
use crate::layout::{Layout, ThreadId, RowId, BoxListKey, SpanRange};
use crate::render::{DrawCommand, Color, Region, SimpleRegion};

pub struct View {
    cursor: (f64, f64),
    cursor_down: Option<(f64, f64)>,
    derived: Derived,
    limits: Span,
    span: Span,
}

struct Derived {
    rows: Vec<Row>,
    selection: Option<InternalSelectionInfo>,
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

#[derive(Eq, PartialEq, Copy, Clone)]
pub struct SelectionInfo {
    pub task: TaskId,
    pub name: NameId,
    pub span: Span,
}

#[derive(Copy, Clone)]
struct InternalSelectionInfo {
    key: BoxListKey,
    task: TaskId,
    name: NameId,
    index: usize,
    span: Span,
}

impl View {
    pub fn new(layout: &Layout) -> View {
        let limits = layout.span_discounting_threads();
        let cursor = (0.0, 0.0);
        View {
            cursor,
            cursor_down: None,
            derived: derived(cursor, limits, layout),
            limits,
            span: limits,
        }
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

    pub fn selected_name(&self) -> Option<SelectionInfo> {
        self.derived.selection.map(|info| SelectionInfo {
            name: info.name,
            span: info.span,
            task: info.task,
        } )
    }

    pub fn hover(&mut self, layout: &Layout, coord: (f64, f64)) {
        self.cursor = coord;
        self.derived.selection = find_selection(self.cursor, self.span, &self.derived.rows, layout);
    }

    pub fn set_span(&mut self, layout: &Layout, span: Span) {
        self.span.begin = bounded(self.limits.begin, span.begin, self.limits.end - MIN_WIDTH as u64);
        self.span.end = bounded(self.span.begin + MIN_WIDTH as u64, span.end, self.limits.end);

        self.derived = derived(self.cursor, self.span, layout);
    }

    pub fn scroll(&mut self, layout: &Layout, offset: f64, scale: f64) {
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

        self.derived = derived(self.cursor, self.span, layout);
    }

    pub fn draw_commands(&self) -> Vec<DrawCommand> {
        let mut res = Vec::new();

        if let Some(total) = self.derived.rows.last().map(|r| r.limit) {

            let name_highlight = if let Some(selection) = self.derived.selection {
                Some((selection.name, Color { r: 0.0, g: 0.0, b: 1.0, a: 1.0 }))
            } else {
                None
            };

            for row in &self.derived.rows {
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
                        name_highlight,
                        region,
                    });

                    if let Some(selection) = self.derived.selection {
                        if selection.key == subrow.key {
                            res.push(DrawCommand::BoxList {
                                key: selection.key,
                                color: Color { r: 1.0, g: 0.0, b: 0.0, a: 1.0 },
                                range: SpanRange { begin: selection.index, end: selection.index + 1 },
                                name_highlight: None,
                                region,
                            })
                        }
                    }
                }
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

        res
    }
}

fn rows(span: Span, layout: &Layout) -> Vec<Row> {
    let mut res = Vec::new();
    let mut base = 0.0;

    for (tid, t) in layout.threads.iter().enumerate() {
        for (rid, r) in t.rows.iter().enumerate() {
            let mut subrows = Vec::new();

            for (ch, val, green) in &[(&r.back, true, 0.5), (&r.fore, false, 1.0)] {
                if ch.has_overlap(span) {
                    subrows.push(Subrow {
                        key: BoxListKey(ThreadId(tid), RowId(rid), *val),
                        color: Color { r: 0.0, g: *green, b: 0.0, a: 1.0 },
                        // TODO: sub-select a range
                        range: SpanRange { begin: 0, end: ch.begins.len() },
                    });
                }
            }

            if subrows.len() > 0 {
                res.push(Row {
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

fn derived(cursor: (f64, f64), span: Span, layout: &Layout) -> Derived {
    let rows = rows(span, layout);

    let selection = find_selection(cursor, span, &rows, layout);

    Derived {
        rows,
        selection,
    }
}

struct Subrow {
    key: BoxListKey,
    range: SpanRange,
    color: Color,
}

struct Row {
    subrows: Vec<Subrow>,
    base: f32,
    limit: f32,
}
