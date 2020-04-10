
use crate::db::{Database, TaskId, Task, Span, NameId};

pub struct Layout {
    pub threads: Vec<Thread>,
}

pub struct Thread {
    pub rows: Vec<Row>,
}

#[derive(Copy, Clone)]
pub struct SpanRange {
    pub begin: usize,
    pub end: usize,
}

impl Thread {
    fn find_row(&mut self, span: Span, is_thread: bool) -> RowId {
        if !is_thread {
            for (id, row) in self.rows.iter().enumerate() {
                if !row.back.has_overlap(span) && !row.fore.has_overlap(span) && !row.reserve.has_overlap(span) {
                    return RowId(id);
                }
            }
        }
        let id = self.rows.len();
        self.rows.push(Row {
            is_thread,
            fore: Chunk::new(),
            back: Chunk::new(),
            reserve: Chunk::new()
        });
        RowId(id)
    }
}

pub struct Row {
    is_thread: bool,
    pub fore: Chunk,
    pub back: Chunk,
    reserve: Chunk,
}

impl Row {
    fn add(&mut self, task: &Task) {
        if let Some(on_cpu) = task.on_cpu.as_ref() {
            self.back.add(task.span, task.name, task.id);
            assert!(!self.fore.has_overlap(task.span));
            
            for span in on_cpu {
                self.fore.add(*span, task.name, task.id);
            }
        } else {
            self.fore.add(task.span, task.name, task.id);
            assert!(!self.back.has_overlap(task.span));
        }
    }
}

#[derive(Clone)]
pub struct Chunk {
    pub begins: Vec<u64>,
    pub ends: Vec<u64>,
    pub names: Vec<NameId>,
    tasks: Vec<TaskId>,
}

impl Chunk {
    fn new() -> Chunk {
        Chunk {
            begins: Vec::new(),
            ends: Vec::new(),
            names: Vec::new(),
            tasks: Vec::new(),
        }
    }

    pub fn has_overlap(&self, span: Span) -> bool {
        let index = match self.ends.binary_search(&span.begin) {
            Ok(index) => index + 1,
            Err(index) => index,
        };
        if index == self.ends.len() {
            false
        } else {
            self.begins[index] < span.end
        }
    }

    pub fn find(&self, val: u64) -> Option<usize> {
        let index = match self.ends.binary_search(&val) {
            Ok(index) => index + 1,
            Err(index) => index,
        };
        if index == self.ends.len() {
            None
        } else if self.begins[index] <= val {
            Some(index)
        } else {
            None
        }
    }

    fn add(&mut self, span: Span, nid: NameId, tid: TaskId) {
        let index = match self.ends.binary_search(&span.begin) {
            Ok(index) => index + 1,
            Err(index) => index,
        };

        if index == self.ends.len() {
            self.begins.push(span.begin);
            self.ends.push(span.end);
            self.names.push(nid);
            self.tasks.push(tid);
        } else {
            assert!(self.begins[index] >= span.end);

            self.begins.insert(index, span.begin);
            self.ends.insert(index, span.end);
            self.names.insert(index, nid);
            self.tasks.insert(index, tid);
        }
    }

    fn spans<'a>(&'a self) -> ChunkSpanIter<'a> {
        ChunkSpanIter {
            begins: &self.begins,
            ends: &self.ends,
            names: &self.names,
        }
    }
}

#[test]
fn test_has_overlap() {
    let chunk = Chunk {
        begins: vec![1, 3, 10],
        ends:   vec![2, 5, 15],
        names: vec![NameId(1), NameId(1), NameId(1)],
        tasks: vec![TaskId(1), TaskId(2), TaskId(3)],
    };
    assert!(chunk.has_overlap(Span { begin: 0, end: 20 }));
    assert!(!chunk.has_overlap(Span { begin: 2, end: 3 }));
    assert!(chunk.has_overlap(Span { begin: 2, end: 4 }));
    assert!(chunk.has_overlap(Span { begin: 4, end: 5 }));
    assert!(chunk.has_overlap(Span { begin: 4, end: 6 }));
    assert!(!chunk.has_overlap(Span { begin: 6, end: 7 }));
    assert!(!chunk.has_overlap(Span { begin: 17, end: 30 }));

    let mut c = chunk.clone();
    c.add(Span { begin: 2, end: 3 }, NameId(2), TaskId(5));
    assert_eq!(c.begins, vec![1, 2, 3, 10]);
    assert_eq!(c.ends,   vec![2, 3, 5, 15]);
}

pub struct ChunkSpanIter<'a> {
    begins: &'a [u64],
    ends: &'a [u64],
    names: &'a [NameId],
}

impl<'a> Iterator for ChunkSpanIter<'a> {
    type Item = (NameId, Span);
    fn next(&mut self) -> Option<(NameId, Span)> {
        if self.begins.len() > 0 {
            let res = (
                self.names[0],
                Span { begin: self.begins[0], end: self.ends[0] },
            );
            self.begins = &self.begins[1..];
            self.ends = &self.ends[1..];
            self.names = &self.names[1..];
            Some(res)
        } else {
            None
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ThreadId(pub usize);

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct RowId(pub usize);

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct BoxListKey(pub ThreadId, pub RowId, pub bool);

struct RowAssignment {
    // thread: ThreadId,
    // row: RowId,
    children: Option<RowId>,
}

pub struct LayoutBuilder {
    children: Vec<Vec<TaskId>>,
    task_to_thread: Vec<ThreadId>,
    threads: Vec<Thread>,
    assignments: Vec<RowAssignment>,
}

impl LayoutBuilder {
    fn add(&mut self, task: &Task) {
        let thread_id = if let Some(parent) = task.parent {
            self.task_to_thread[parent.0 as usize]
        } else {
            let thread_id = ThreadId(self.threads.len());
            self.threads.push(Thread {
                rows: Vec::new(),
            });
            thread_id
        };
        assert!(self.task_to_thread.len() == task.id.0 as usize);
        self.task_to_thread.push(thread_id);

        let thread = &mut self.threads[thread_id.0];

        let row = if let Some(parent) = task.parent {
            let row_id = self.assignments[parent.0 as usize].children.unwrap();
            let row = &thread.rows[row_id.0];
            if row.fore.has_overlap(task.span) || row.back.has_overlap(task.span) {
                thread.find_row(task.span, task.parent.is_none())
            } else {
                row_id
            }
        } else {
            thread.find_row(task.span, task.parent.is_none())
        };

        thread.rows[row.0].add(task);

        let children = if self.children[task.id.0 as usize].len() > 0 {
            let child_row = thread.find_row(task.span, task.parent.is_none());
            thread.rows[child_row.0].reserve.add(task.span, task.name, task.id);
            Some(child_row)
        } else {
            None
        };

        assert!(self.assignments.len() == task.id.0 as usize);
        self.assignments.push(RowAssignment {
            // thread: thread_id,
            // row,
            children,
        });
    }
}

impl Layout {
    pub fn new(db: &Database) -> Layout {

        let mut children = Vec::new();
        for task in &db.tasks {
            children.push(Vec::new());
            if let Some(parent) = task.parent {
                children[parent.0 as usize].push(task.id);
            }
        }

        let mut b = LayoutBuilder {
            children,
            task_to_thread: Vec::new(),
            threads: Vec::new(),
            assignments: Vec::new(),
        };

        for task in &db.tasks {
            b.add(task)
        }

        Layout {
            threads: b.threads,
        }
    }

    pub fn span_discounting_threads(&self) -> Span {
        let mut begin = std::u64::MAX;
        let mut end = 0;
        for t in &self.threads {
            for row in &t.rows {
                if !row.is_thread {
                    begin = std::cmp::min(
                        begin,
                        *row.fore.begins.iter().chain(row.back.begins.iter()).min().unwrap());

                    end = std::cmp::max(
                        end,
                        *row.fore.ends.iter().chain(row.back.ends.iter()).max().unwrap());
                }
            }
        }
        Span {
            begin,
            end
        }
    }

    pub fn iter_box_lists(&self) -> impl Iterator<Item=(BoxListKey, ChunkSpanIter)> {
        self.threads.iter().enumerate().flat_map(|(tid, t)| {
            t.rows.iter().enumerate().flat_map(move |(rid, r)| {
                let mut res = Vec::new();
                if r.fore.begins.len() > 0 {
                    res.push((BoxListKey(ThreadId(tid), RowId(rid), false), r.fore.spans()));
                }
                if r.back.begins.len() > 0 {
                    res.push((BoxListKey(ThreadId(tid), RowId(rid), true), r.back.spans()));
                }
                res.into_iter()
            })
        })
    }
}
