use crate::db::Span;
use crate::layout::{Layout, ThreadId, RowId, BoxListKey, SpanRange};
use crate::render::{DrawCommand, Color, Region, SimpleRegion};

pub struct View {
    cursor: (f64, f64),
    rows: Vec<Row>,
    limits: Span,
    smallest_span_len: u64,
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

fn lerp(a: f64, b: f64, factor: f64) -> f64 {
    a * (1.0 - factor) + b * factor
}

impl View {
    pub fn new(layout: &Layout) -> View {
        let limits = layout.span_discounting_threads();
        View {
            cursor: (0.0, 0.0),
            rows: rows(limits, layout),
            limits,
            smallest_span_len: std::cmp::max(1, layout.smallest_span_len()),
            span: limits,
        }
    }

    pub fn hover(&mut self, coord: (f64, f64)) {
        self.cursor = coord;
    }

    pub fn scroll(&mut self, layout: &Layout, offset: f64, scale: f64) {

        let factor = 1.05f64.powf(-scale / 1e1);

        let cursor = self.cursor.0;
        let begin = self.span.begin as f64;
        let end = self.span.end as f64;

        let mut new_width = (end - begin) * factor;

        let min_width = 1e5;
        let max_width = (self.limits.end - self.limits.begin) as f64;

        if new_width < min_width {
            new_width = min_width;
        }

        if new_width > max_width {
            new_width = max_width;
        }

        let new_begin = lerp(begin, end - new_width, cursor);
        let new_end = new_begin + new_width;

        self.span.begin = bounded(self.limits.begin, new_begin as u64, self.limits.end - min_width as u64);
        self.span.end = bounded(self.span.begin + min_width as u64, new_end as u64, self.limits.end);

        self.rows = rows(self.span, layout);
    }

    pub fn draw_commands(&self, layout: &Layout) -> Vec<DrawCommand> {
        let mut res = Vec::new();

        if let Some(total) = self.rows.last().map(|r| r.limit) {
            for row in &self.rows {
                res.push(DrawCommand::BoxList {
                    key: row.key,
                    color: row.color,
                    range: row.range,
                    region: Region {
                        logical_base: (self.span.begin as f32) / 1e9,
                        logical_limit: (self.span.end as f32) / 1e9,

                        vertical_base: row.base / total,
                        vertical_limit: row.limit / total,
                    },
                })
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
            for (ch, val, alpha) in &[(&r.back, true, 0.5), (&r.fore, false, 1.0)] {
                if ch.has_overlap(span) {
                    res.push(Row {
                        key: BoxListKey(ThreadId(tid), RowId(rid), *val),
                        color: Color { r: 1.0, g: 0.0, b: 0.0, a: *alpha },
                        // TODO: sub-select a range
                        range: SpanRange { begin: 0, end: ch.begins.len() },
                        base,
                        limit: base + 1.0,
                    });
                    base += 1.0;
                }
            }
        }
    }

    res
}

struct Row {
    key: BoxListKey,
    range: SpanRange,
    color: Color,
    base: f32,
    limit: f32,
}
