use std::cell::RefCell;
use std::time::{Duration, Instant, SystemTime};

use event::{SpanId, TraceEvent};

thread_local! {
    pub static TRACER_STATE: RefCell<TracerState> = RefCell::new(TracerState::default());
}
lazy_static! {
    static ref EPOCH: (SystemTime, Instant) = (SystemTime::now(), Instant::now());
}

pub trait Logger {
    fn write(&mut self, event: TraceEvent);
}

pub struct DebugLogger;
impl Logger for DebugLogger {
    fn write(&mut self, event: TraceEvent) {
        eprintln!("{:?}", event);
    }
}

pub struct TracerState {
    pub current_span: Option<SpanId>,
    pub currently_logging_wakeup: bool,

    pub writer: Option<Box<Logger>>,

    start: Instant,
    since_epoch: Duration,
}

impl Default for TracerState {
    fn default() -> Self {
        let (_, epoch) = *EPOCH;
        let now = Instant::now();
        TracerState {
            current_span: None,
            currently_logging_wakeup: false,
            writer: None,

            since_epoch: now.duration_since(epoch),
            start: now,
        }
    }
}

impl TracerState {
    pub fn start(&mut self, writer: Box<Logger>) {
        assert!(self.writer.is_none());
        self.writer = Some(writer);
    }

    pub fn emit(&mut self, event: TraceEvent) {
        if let Some(ref mut w) = self.writer.as_mut() {
            w.write(event);
        }
    }

    pub fn now(&self) -> Duration {
        // Duration relative to thread start + relative to process start
        Instant::now().duration_since(self.start) + self.since_epoch
    }
}
