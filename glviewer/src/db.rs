use std::collections::HashMap;
use std::io::{BufReader, BufRead};
use std::fs::File;
use bit_set::BitSet;
use cyclotron_backend::TraceEvent as JsonTraceEvent;
use std::path::Path;

#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub struct Span {
    pub begin: u64,
    pub end: u64,
}

impl Span {
    pub fn intersects(self, other: Span) -> bool {
        self.begin < other.end && self.end > other.begin
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub struct NameId(pub u32);

#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub struct TaskId(pub u32);

pub struct Task {
    pub id: TaskId,
    pub parent: Option<TaskId>,
    pub name: NameId,
    pub span: Span,
    pub on_cpu: Option<Vec<Span>>,
}

struct NameTable {
    by_name: HashMap<String, NameId>,
    names: Vec<String>,
}

impl NameTable {
    fn new() -> NameTable {
        NameTable {
            by_name: HashMap::new(),
            names: Vec::new(),
        }
    }

    fn insert(&mut self, name: String) -> NameId {
        let names = &mut self.names;
        *self.by_name.entry(name.clone()).or_insert_with(|| {
            let id = NameId(names.len() as u32);
            names.push(name);
            id
        })
    }
}

pub struct Database {
    pub tasks: Vec<Task>,
}

impl Database {
    pub fn load(path: impl AsRef<Path>) -> Database {
        let mut closed = BitSet::new();
        let mut tasks = Vec::new();
        let mut unterminated = Vec::new();
        let mut task_ids = HashMap::new();
        let mut names = NameTable::new();
        let mut file = BufReader::new(File::open(path).unwrap());

        loop {
            let mut buf = String::new();
            let num_read = file.read_line(&mut buf).unwrap();

            if num_read == 0 || !buf.ends_with("\n") {
                break;
            } else {
                buf.pop();
                match serde_json::from_str(&buf).unwrap() {
                    JsonTraceEvent::AsyncStart { id, ts, name, parent_id, metadata } => {
                        let tid = TaskId(task_ids.len() as u32);
                        assert!(task_ids.insert(id, tid).is_none());
                        let parent = task_ids[&parent_id];
                        tasks.push(Task {
                            id: tid,
                            parent: Some(parent),
                            name: names.insert(name),
                            span: Span { begin: ts.as_nanos() as u64, end: std::u64::MAX },
                            on_cpu: Some(Vec::new()),
                        });
                        unterminated.push(None);
                    }
                    JsonTraceEvent::AsyncOnCPU { id, ts } => {
                        let tid = task_ids[&id];
                        assert!(std::mem::replace(&mut unterminated[tid.0 as usize], Some(ts.as_nanos() as u64)).is_none());
                    }
                    JsonTraceEvent::AsyncOffCPU { id, ts,  } => {
                        let tid = task_ids[&id];
                        let begin = unterminated[tid.0 as usize].take().unwrap();
                        let end = ts.as_nanos() as u64;
                        tasks[tid.0 as usize].on_cpu.as_mut().unwrap().push(Span { begin, end });
                    }
                    JsonTraceEvent::AsyncEnd { id, ts, outcome } => {
                        let tid = task_ids[&id];
                        assert!(!closed.contains(tid.0 as usize));
                        closed.insert(tid.0 as usize);
                        tasks[tid.0 as usize].span.end = ts.as_nanos() as u64;
                    }
                    JsonTraceEvent::SyncStart { id, ts, name, parent_id, metadata } => {
                        let tid = TaskId(task_ids.len() as u32);
                        assert!(task_ids.insert(id, tid).is_none());
                        let parent = task_ids[&parent_id];
                        tasks.push(Task {
                            id: tid,
                            parent: Some(parent),
                            name: names.insert(name),
                            span: Span { begin: ts.as_nanos() as u64, end: std::u64::MAX },
                            on_cpu: None,
                        });
                        unterminated.push(None);
                    }
                    JsonTraceEvent::SyncEnd { id, ts } => {
                        let tid = task_ids[&id];
                        assert!(!closed.contains(tid.0 as usize));
                        closed.insert(tid.0 as usize);
                        tasks[tid.0 as usize].span.end = ts.as_nanos() as u64;
                    }
                    JsonTraceEvent::ThreadStart { id, ts, name } => {
                        let tid = TaskId(task_ids.len() as u32);
                        assert!(task_ids.insert(id, tid).is_none());
                        tasks.push(Task {
                            id: tid,
                            parent: None,
                            name: names.insert(name),
                            span: Span { begin: ts.as_nanos() as u64, end: std::u64::MAX },
                            on_cpu: None,
                        });
                        unterminated.push(None);
                    }
                    JsonTraceEvent::ThreadEnd { id, ts } => {
                        let tid = task_ids[&id];
                        assert!(!closed.contains(tid.0 as usize));
                        closed.insert(tid.0 as usize);
                        tasks[tid.0 as usize].span.end = ts.as_nanos() as u64;
                    }
                    JsonTraceEvent::Wakeup { waking_span, parked_span, ts } => {}
                }
            }
        }

        Database {
            tasks,
        }
    }
}