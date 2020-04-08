use crate::db::Span;
use crate::layout::Layout;

pub struct View {
    cursor: (f64, f64),
    span: Span,
}

impl View {
    pub fn new(layout: &Layout) -> View {
        View {
            cursor: (0.0, 0.0),
            span: layout.span_discounting_threads(),
        }
    }

    pub fn hover(&mut self, coord: (f64, f64)) {
        self.cursor = coord;
    }

    pub fn scroll(&mut self, offset: f64, scale: f64) {

    }
}