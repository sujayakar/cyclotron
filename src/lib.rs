extern crate futures;
#[macro_use]
extern crate lazy_static;
extern crate rand;
extern crate serde;
extern crate serde_json;
#[macro_use]
extern crate serde_derive;

use std::sync::Arc;
use std::thread;
use std::ops::{
	Deref,
	DerefMut,
};
use futures::{
	Async,
	Future,
	Poll,
};
use futures::task::AtomicTask;
use futures::executor::{Notify, NotifyHandle, spawn};
use std::time::Instant;

use std::cell::RefCell;

type SpanId = u64;

thread_local! {
	static CURRENT_SPAN: RefCell<Option<SpanId>> = RefCell::new(None);
	static LOGGING_WAKEUP: RefCell<bool> = RefCell::new(false);
}
lazy_static! {
	static ref EPOCH: Instant = Instant::now();
}
fn since_epoch(t: Instant) -> f64 {
	let duration = t.duration_since(*EPOCH);
	(duration.as_secs() as f64) + duration.subsec_nanos() as f64 * 1e-9
}

pub trait TraceFuture: Future + Sized {
	fn traced<S: Into<String>>(self, name: S) -> TracedFuture<Self> {
		TracedFuture {
			name: name.into(),
			span_id: rand::random(),
			execution: None,
			inner: Some(self),
		}
	}
}
impl<F: Future + Sized> TraceFuture for F {}

#[derive(Debug)]
struct Execution {
	started: Instant,
	parent: Option<SpanId>,
	on_cpu: Vec<(Instant, Instant)>,

	finished: Option<Instant>,
	success: Option<bool>,
}

pub struct TracedFuture<F> {
	name: String,
	span_id: SpanId,
	execution: Option<Execution>,
	inner: Option<F>,
}

#[derive(Serialize, Deserialize)]
pub struct Span {
	name: String,
	id: SpanId,
	parent_id: Option<SpanId>,

	started: f64,
	finished: f64,
	on_cpu: Vec<(f64, f64)>,
	success: bool,
}


#[derive(Serialize, Deserialize)]
pub struct SyncSpanEvent {
	name: String,
	thread_name: String,
	id: SpanId,
	parent_id: Option<SpanId>,

	started: f64,
	finished: f64,
}

pub struct SyncSpan {
	name: String,
	span_id: SpanId,
	parent: Option<SpanId>,
	started: Instant,
}

impl SyncSpan {
	pub fn new(name: String) -> Self {
		let _ = *EPOCH;
		CURRENT_SPAN.with(|span_cell| {
			let span = SyncSpan {
				name: name,
				span_id: rand::random(),
				parent: *span_cell.borrow(),
				started: Instant::now(),
			};
			*span_cell.borrow_mut() = Some(span.span_id);
			span
		})
	}
}

impl Drop for SyncSpan {
	fn drop(&mut self) {
		CURRENT_SPAN.with(|span_cell| {
			*span_cell.borrow_mut() = self.parent;
		});
		let span = SyncSpanEvent {
			name: self.name.clone(),
			thread_name: thread::current().name().unwrap_or("UNKNOWN").to_owned(),
			id: self.span_id,
			parent_id: self.parent,
			started: since_epoch(self.started),
			finished: since_epoch(Instant::now()),
		};
		eprintln!("{}", serde_json::to_string(&span).unwrap());
	}
}

impl<F> TracedFuture<F> {
	pub fn into_inner(mut self) -> F {
		self.inner.take().unwrap()
	}

	fn emit(&mut self) {
		if let Some(execution) = self.execution.take() {
			let span = Span {
				name: self.name.clone(),
				id: self.span_id,
				parent_id: execution.parent,
				started: since_epoch(execution.started),
				finished: since_epoch(execution.finished.unwrap()),
				on_cpu: execution.on_cpu.iter()
					.map(|&(a, b)| (since_epoch(a), since_epoch(b)))
					.collect(),
				success: execution.success.unwrap(),
			};
			eprintln!("{}", serde_json::to_string(&span).unwrap());
		}
	}
}

impl<F> Deref for TracedFuture<F> {
	type Target = F;
	fn deref(&self) -> &F {
		self.inner.as_ref().unwrap()
	}
}

impl<F> DerefMut for TracedFuture<F> {
	fn deref_mut(&mut self) -> &mut F {
		self.inner.as_mut().unwrap()
	}
}

impl<F> Drop for TracedFuture<F> {
	fn drop(&mut self) {
		if let Some(ref mut execution) = self.execution {
			if execution.finished.is_none() {
				execution.finished = Some(Instant::now());
			}
			if execution.success.is_none() {
				execution.success = Some(false);
			}
		}
		self.emit();
	}
}

struct Notifier {
	parent_task: AtomicTask,
	blocking_span: SpanId,
}

impl Notifier {
	fn new(span: SpanId) -> Self {
		Notifier {
			parent_task: AtomicTask::new(),
			blocking_span: span,
		}
	}
}

#[derive(Serialize, Deserialize)]
pub struct Wakeup {
	waking_span: SpanId,
	blocking_span: SpanId,
	ts: f64,
}

impl Notify for Notifier {
	fn notify(&self, _: usize) {
		LOGGING_WAKEUP.with(|logging_cell| {
			if *logging_cell.borrow() {
				self.parent_task.notify();
				return;
			}

			*logging_cell.borrow_mut() = true;
			self.parent_task.notify();
			*logging_cell.borrow_mut() = false;

			CURRENT_SPAN.with(|span_cell| {
				let span = span_cell.borrow();
				if let Some(span) = *span {
					let evt = Wakeup {
						waking_span: span,
						blocking_span: self.blocking_span,
						ts: since_epoch(Instant::now()),
					};
					eprintln!("{}", serde_json::to_string(&evt).unwrap());
				}
			});
		})
	}
}


impl<F: Future> Future for TracedFuture<F> {
	type Item = F::Item;
	type Error = F::Error;
	fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
		CURRENT_SPAN.with(|span_cell| {
			let parent_span = *span_cell.borrow();
			if let Some(ref execution) = self.execution {
				// Sanity check that our parent hasn't changed
				assert_eq!(execution.parent, parent_span);
			} else {
				let _ = *EPOCH;
				self.execution = Some(Execution {
					started: Instant::now(),
					parent: parent_span,
					on_cpu: vec![],
					finished: None,
					success: None,
				});
			}

			// NB: Not panic safe yet.
			let notifier = Notifier::new(self.span_id);
			notifier.parent_task.register();
			let handle = NotifyHandle::from(Arc::new(notifier));

			*span_cell.borrow_mut() = Some(self.span_id);
			let start = Instant::now();
			let result = {
				let mut f = spawn(self.inner.as_mut().unwrap());
				f.poll_future_notify(&handle, 0)
			};
			let end = Instant::now();
			*span_cell.borrow_mut() = parent_span;

			let should_emit = {
				let mut execution = self.execution.as_mut().unwrap();
				execution.on_cpu.push((start, end));
				match result {
					Ok(Async::Ready(..)) | Err(..) => {
						execution.finished = Some(Instant::now());
						execution.success = Some(result.is_ok());
						true
					},
					Ok(Async::NotReady) => {
						// Okay, we're blocking, so we should have parked the current task.
						false
					},
				}
			};
			if should_emit {
				self.emit();
			}
			result
		})
	}
}
