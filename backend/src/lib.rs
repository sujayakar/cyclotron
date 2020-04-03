extern crate futures;
extern crate rand;
extern crate serde;
extern crate serde_json;
#[macro_use]
extern crate lazy_static;
#[allow(unused_imports)]
#[macro_use]
extern crate serde_derive;

mod async;
mod event;
mod state;
mod sync;
pub mod json;

pub use async::{TraceFuture, TracedFuture};
pub use event::{TraceEvent, SpanId, AsyncOutcome};
pub use sync::{TracedThread, SyncSpan};
pub use state::{DebugLogger, NoopLogger, Logger};

#[cfg(test)]
mod tests;
