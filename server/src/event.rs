use serde_json;
use std::time::Duration;
use std::collections::{
    HashMap,
    HashSet,
};

// Copied from dropbox/cyclotron/src/event.rs

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Deserialize, Serialize)]
pub struct SpanId(pub u64);

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum AsyncOutcome {
    Success,
    Cancelled,
    Error(String),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum TraceEvent {
    /// Logged the first time a future is polled after a logger is installed.  If this is the first
    /// time the future is *ever* polled, `is_restart` will be false.
    AsyncStart {
        name: String,
        id: SpanId,
        parent_id: SpanId,
        ts: Duration,
        metadata: serde_json::Value,
        is_restart: bool,
    },
    /// Logged immediately before each time the future is polled
    AsyncOnCPU { id: SpanId, ts: Duration },
    /// Logged immediately after each time the future is polled
    AsyncOffCPU { id: SpanId, ts: Duration },
    /// Logged when the future is completed. Returning `Ok(Async::Ready(..))` will set
    /// `AsyncOutcome::Success`, `Err(e)` will set `AsyncOutcome::Error`, and dropping the future
    /// will set `AsyncOutcome::Cancelled`.
    AsyncEnd {
        id: SpanId,
        ts: Duration,
        outcome: AsyncOutcome,
    },

    /// Logged when a sync span is entered.  Note that since we don't repeatedly
    /// poll synchronous spans, we don't make an attempt to restart them when
    /// the logger changes.
    SyncStart {
        name: String,
        id: SpanId,
        parent_id: SpanId,
        ts: Duration,
        metadata: serde_json::Value,
    },
    /// Logged when a sync span is exited and the current generation matches the
    /// one at the span's start.
    SyncEnd { id: SpanId, ts: Duration },

    /// Logged when a logger is installed on a thread.  If this corresponds with thread creation,
    /// `is_restart` will be set to false.
    ThreadStart {
        name: String,
        id: SpanId,
        ts: Duration,
        is_restart: bool,
    },
    /// Logged when a thread is dropped.
    ThreadEnd { id: SpanId, ts: Duration },

    /// Logged when a wakeup originates from a traced thread, noting the current span and span that's
    /// being woken up
    Wakeup {
        waking_span: SpanId,
        parked_span: SpanId,
        ts: Duration,
    },
}

#[derive(Clone, Eq, Hash)]
struct EventResult {
    buf: String, // buffer before json conversion; list includes e.g. both AsyncStart and AsyncEnd
    ts: Duration, // the ts from self.event, extracted for convenient sorting
}

// Allow sorting by timestamp.
impl Ord for EventResult {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering { self.ts.cmp(&other.ts) }
}
impl PartialOrd for EventResult {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> { Some(self.ts.cmp(&other.ts)) }
}
impl PartialEq for EventResult {
    fn eq(&self, other: &Self) -> bool { self.ts == other.ts }
}

#[derive(Clone)]
struct EventNobe {
    events: Vec<EventResult>,
    name: String,
    parent: Option<SpanId>,
    children: Vec<SpanId>,
}

#[derive(Eq, PartialEq, Ord, PartialOrd, Hash)]
struct Wakeup {
    event: EventResult,
    waking_span: SpanId,
    parked_span: SpanId,
}

pub struct EventTree {
    slab: HashMap<SpanId, EventNobe>,
    roots: HashSet<SpanId>,
    // emit these in postprocessing, if both nodes are in the tree
    wakeups: HashSet<Wakeup>,
    // what we're gonna filter for
    goal_names: HashSet<String>,
    goal_spans: HashSet<SpanId>,
}

impl EventTree {
    pub fn new(goals: Vec<&str>) -> Self {
        let mut goal_names = HashSet::new();
        for goal in goals {
            goal_names.insert(goal.to_string());
        }
        EventTree {
            slab: HashMap::new(),
            roots: HashSet::new(),
            wakeups: HashSet::new(),
            goal_names,
            goal_spans: HashSet::new(),
        }
    }

    fn add_nobe(&mut self, id: SpanId, buf: String, name: String, ts: Duration, parent: Option<SpanId>) -> Result<(), (failure::Error, String)> {
        if self.slab.contains_key(&id) {
            return Err((failure::format_err!("duplicate nobe"), buf));
        }
        if self.goal_names.contains(&name) {
            self.goal_spans.insert(id);
        }
        self.slab.insert(id, EventNobe {
            events: vec![EventResult { buf, ts }],
            name,
            parent,
            children: vec![],
        });
        Ok(())
    }

    pub fn add(&mut self, buf: String) -> Result<(), (failure::Error, String)> {
        let event: TraceEvent = match serde_json::from_str(&buf) {
            Ok(event) => event,
            Err(e) => return Err((e.into(), buf)),
        };
        match event {
            // Add new root.
            TraceEvent::ThreadStart { id, name, ts, .. } => {
                self.add_nobe(id, buf, name, ts, None)?;
                self.roots.insert(id);
            }

            // Add new nobe with a parent.
            TraceEvent::AsyncStart { id, parent_id, name, ts, .. }
            | TraceEvent::SyncStart { id, parent_id, name, ts, .. } => {
                assert!(!self.slab.contains_key(&id), "duplicate nobe");
                if let Some(parent_nobe) = self.slab.get_mut(&parent_id) {
                    parent_nobe.children.push(id);
                    self.add_nobe(id, buf, name, ts, Some(parent_id))?;
                } else {
                    println!("warning: parentless nobe {:?} (alleged parent: {:?}); treating as root", id, parent_id);
                    self.add_nobe(id, buf, name, ts, None)?;
                    self.roots.insert(id);
                }
            },

            // Add event to existing nobe in the tree.
            TraceEvent::AsyncOnCPU { id, ts, .. }
            | TraceEvent::AsyncOffCPU { id, ts, .. }
            | TraceEvent::AsyncEnd { id, ts, .. }
            | TraceEvent::SyncEnd { id, ts, .. }
            | TraceEvent::ThreadEnd { id, ts, .. } => {
                let nobe = self.slab.get_mut(&id).expect("nobeless event");
                nobe.events.push(EventResult { buf, ts });
            }

            // Add new wakeup.
            TraceEvent::Wakeup { waking_span, parked_span, ts, .. } => {
                self.wakeups.insert(Wakeup { event: EventResult { buf, ts }, waking_span, parked_span });
            }
        }
        Ok(())
    }

    // TODO: be able to do this filter in-line
    // Guaranteed to return in root-first order (parence before children), and wakeups last, i guess.
    pub fn filter(&self) -> Vec<String> {
        let mut seen_ids = HashSet::new();
        let mut result = vec![];
        for id in &self.goal_spans {
            let nobe = self.slab.get(id).expect("this nobe missing during filter");
            // Process this nobe's ancestors.
            self.add_ancestors(&mut seen_ids, &mut result, nobe.parent);
            // Add all its children, and children's children, and so on.
            // NB this includes adding the node itself
            self.add_children(&mut seen_ids, &mut result, *id);
        }
        for wakeup in &self.wakeups {
            // Add wakeup only if both of its endpoints are included in the result.
            if seen_ids.contains(&wakeup.waking_span) && seen_ids.contains(&wakeup.parked_span) {
                println!("adding wakeup: {}", wakeup.event.buf);
                result.push(wakeup.event.clone());
            }
        }
        result.sort();
        result.into_iter().map(|x| x.buf).collect()
    }

    fn add_ancestors(&self, seen_ids: &mut HashSet<SpanId>, result: &mut Vec<EventResult>, ancestor_id: Option<SpanId>) {
        if let Some(id) = ancestor_id {
            if !seen_ids.contains(&id) {
                let nobe = self.slab.get(&id).expect("ancestor nobe missing");
                //println!("adding {} evence from nobe named '{}'", nobe.events.len(), nobe.name);
                seen_ids.insert(id);
                // Add after iterating, to ensure parent-first order.
                self.add_ancestors(seen_ids, result, nobe.parent);
                for event in &nobe.events {
                    result.push(event.clone());
                }
            }
        }
    }

    fn add_children(&self, seen_ids: &mut HashSet<SpanId>, result: &mut Vec<EventResult>, id: SpanId) {
        if !seen_ids.contains(&id) {
            // Add before iterating, to ensure parent-first order.
            let nobe = self.slab.get(&id).expect("child nobe missing");
            //println!("adding {} evence from nobe named '{}'", nobe.events.len(), nobe.name);
            seen_ids.insert(id);
            for event in &nobe.events {
                result.push(event.clone());
            }
            for child in &nobe.children {
                self.add_children(seen_ids, result, *child);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::EventTree;

    fn buf_thread_start(name: &str, id: usize) -> String {
        format!("{{\"ThreadStart\":{{\"name\":\"{}\",\"id\":{},\"ts\":{{\"secs\":0,\"nanos\":0}},\"is_restart\":false}}}}", name, id)
    }

    fn buf_sync_start(name: &str, id: usize, parent_id: usize) -> String {
        format!("{{\"SyncStart\":{{\"name\":\"{}\",\"id\":{},\"parent_id\":{},\"ts\":{{\"secs\":0,\"nanos\":0}},\"metadata\":null}}}}", name, id, parent_id)
    }

    fn buf_sync_end(id: usize) -> String {
        format!("{{\"SyncEnd\":{{\"id\":{},\"ts\":{{\"secs\":0,\"nanos\":0}}}}}}", id)
    }

    fn buf_wakeup(waking_id: usize, parked_id: usize) -> String {
        format!("{{\"Wakeup\":{{\"waking_span\":{},\"parked_span\":{},\"ts\":{{\"secs\":0,\"nanos\":0}}}}}}", waking_id, parked_id)
    }

    #[test]
    fn test_event_tree_multiple_roots() {
        let mut tree = EventTree::new(vec![]);
        let mut root_id = 0;
        for name in &["John", "Paul", "George", "Ringo"] {
            tree.add(buf_thread_start(name, root_id)).expect("add");
            root_id += 1;
        }
        assert_eq!(tree.roots.len(), 4);
    }

    #[test]
    fn test_event_child_basic() {
        let mut tree = EventTree::new(vec!["Graydon"]);
        tree.add(buf_thread_start("Graydon", 0)).expect("add root");
        tree.add(buf_sync_start("Niko", 1, 0)).expect("add child");
        tree.add(buf_sync_start("Patrick", 2, 0)).expect("add child");
        assert_eq!(tree.filter().len(), 3);
    }

    #[test]
    fn test_event_parent_basic() {
        let mut tree = EventTree::new(vec!["Niko"]);
        tree.add(buf_thread_start("Graydon", 0)).expect("add root");
        tree.add(buf_sync_start("Niko", 1, 0)).expect("add child");
        tree.add(buf_sync_start("Patrick", 2, 0)).expect("add child");
        assert_eq!(tree.filter().len(), 2); // not include patrick
    }

    #[test]
    fn test_event_not_include_duplicates() {
        let mut tree = EventTree::new(vec!["Niko", "Patrick"]);
        tree.add(buf_thread_start("Graydon", 0)).expect("add root");
        tree.add(buf_sync_start("Niko", 1, 0)).expect("add child");
        tree.add(buf_sync_start("Patrick", 2, 0)).expect("add child");
        assert_eq!(tree.filter().len(), 3);
    }

    #[test]
    fn test_event_include_end_span() {
        let mut tree = EventTree::new(vec!["Niko"]);
        tree.add(buf_thread_start("Graydon", 0)).expect("add root");
        tree.add(buf_sync_start("Niko", 1, 0)).expect("add child");
        tree.add(buf_sync_start("Patrick", 2, 0)).expect("add child");
        tree.add(buf_sync_end(2)).expect("add child");
        tree.add(buf_sync_end(1)).expect("add child");
        assert_eq!(tree.filter().len(), 3);
    }

    #[test]
    fn test_event_wakeup_included() {
        let mut tree = EventTree::new(vec!["Niko"]);
        tree.add(buf_thread_start("Graydon", 0)).expect("add root");
        tree.add(buf_sync_start("Niko", 1, 0)).expect("add child");
        tree.add(buf_sync_start("Patrick", 2, 0)).expect("add child");
        tree.add(buf_wakeup(0, 1)).expect("add wakeup");
        assert_eq!(tree.filter().len(), 3);
    }

    #[test]
    fn test_event_wakeup_not_included() {
        let mut tree = EventTree::new(vec!["Niko"]);
        tree.add(buf_thread_start("Graydon", 0)).expect("add root");
        tree.add(buf_sync_start("Niko", 1, 0)).expect("add child");
        tree.add(buf_sync_start("Patrick", 2, 0)).expect("add child");
        tree.add(buf_wakeup(2, 1)).expect("add wakeup");
        assert_eq!(tree.filter().len(), 2);
    }
}
