use crate::db::{Span, NameId};
use crate::layout::{Layout, ThreadId, RowId, BoxListKey, SpanRange};
use crate::render::{DrawCommand, Color, Region};

pub struct View {
    cursor: (f64, f64),
    derived: Derived,
    limits: Span,
    smallest_span_len: u64,
    span: Span,
}

struct Derived {
    rows: Vec<Row>,
    selection: Option<(BoxListKey, NameId, usize)>,
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
        let cursor = (0.0, 0.0);
        View {
            cursor,
            derived: derived(cursor, limits, layout),
            limits,
            smallest_span_len: std::cmp::max(1, layout.smallest_span_len()),
            span: limits,
        }
    }

    pub fn hover(&mut self, layout: &Layout, coord: (f64, f64)) {
        self.cursor = coord;
        self.derived.selection = find_selection(self.cursor, self.span, &self.derived.rows, layout);
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

        self.derived = derived(self.cursor, self.span, layout);
    }

    pub fn draw_commands(&self, layout: &Layout) -> Vec<DrawCommand> {
        let mut res = Vec::new();

        if let Some(total) = self.derived.rows.last().map(|r| r.limit) {

            let name_highlight = if let Some((_, name, _)) = self.derived.selection {
                Some((name, Color { r: 0.0, g: 0.0, b: 1.0, a: 1.0 }))
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

                    if let Some((key, _, index)) = self.derived.selection {
                        if key == subrow.key {
                            res.push(DrawCommand::BoxList {
                                key,
                                color: Color { r: 1.0, g: 0.0, b: 0.0, a: 1.0 },
                                range: SpanRange { begin: index, end: index + 1 },
                                name_highlight: None,
                                region,
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

fn find_selection(cursor: (f64, f64), span: Span, rows: &[Row], layout: &Layout) -> Option<(BoxListKey, NameId, usize)> {
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
                    return Some((key, chunk.names[index], index));
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
