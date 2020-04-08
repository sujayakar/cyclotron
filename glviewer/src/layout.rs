
use crate::db::{Database, TaskId, Task, Span};

pub struct Layout {
    threads: Vec<Thread>,
}

pub struct Thread {
    rows: Vec<Row>,
}

impl Thread {
    fn find_row(&mut self, span: Span) -> RowId {
        let id = self.rows.len();
        for (id, row) in self.rows.iter().enumerate() {
            if !row.back.has_overlap(span) && !row.fore.has_overlap(span) && !row.reserve.has_overlap(span) {
                return RowId(id);
            }
        }
        self.rows.push(Row { fore: Chunk::new(), back: Chunk::new(), reserve: Chunk::new() });
        RowId(id)
    }
}

pub struct Row {
    fore: Chunk,
    back: Chunk,
    reserve: Chunk,
}

impl Row {
    fn add(&mut self, task: &Task) {
        if let Some(on_cpu) = task.on_cpu.as_ref() {
            self.back.add(task.span, task.id);
            assert!(!self.fore.has_overlap(task.span));
            
            for span in on_cpu {
                self.fore.add(*span, task.id);
            }
        } else {
            self.fore.add(task.span, task.id);
            assert!(!self.back.has_overlap(task.span));
        }
    }
}

pub struct Chunk {
    begins: Vec<u64>,
    ends: Vec<u64>,
    tasks: Vec<TaskId>,
}

impl Chunk {
    fn new() -> Chunk {
        Chunk {
            begins: Vec::new(),
            ends: Vec::new(),
            tasks: Vec::new(),
        }
    }

    fn has_overlap(&self, span: Span) -> bool {
        let begin = match self.ends.binary_search(&span.begin) {
            Ok(index) => index,
            Err(index) => {
                if index == self.ends.len() {
                    return false;
                }
                index
            },
        };
        self.begins[begin] <= span.end
    }

    fn add(&mut self, span: Span, tid: TaskId) {

        match self.ends.binary_search(&span.begin) {
            Ok(index) => panic!(),
            Err(index) => {
                if index == self.ends.len() {
                    self.begins.push(span.begin);
                    self.ends.push(span.end);
                    self.tasks.push(tid);
                } else {
                    assert!(self.begins[index] > span.end);

                    self.begins.insert(index, span.begin);
                    self.ends.insert(index, span.end);
                    self.tasks.insert(index, tid);
                }
            },
        };
    }

    fn spans<'a>(&'a self) -> ChunkSpanIter<'a> {
        ChunkSpanIter {
            begins: &self.begins,
            ends: &self.ends,
        }
    }
}

pub struct ChunkSpanIter<'a> {
    begins: &'a [u64],
    ends: &'a [u64],
}

impl<'a> Iterator for ChunkSpanIter<'a> {
    type Item = Span;
    fn next(&mut self) -> Option<Span> {
        if self.begins.len() > 0 {
            let res = Span { begin: self.begins[0], end: self.ends[0] };
            self.begins = &self.begins[1..];
            self.ends = &self.ends[1..];
            Some(res)
        } else {
            None
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub struct ThreadId(usize);

#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub struct RowId(usize);

#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub struct BoxListKey(pub ThreadId, pub RowId, pub bool);

struct RowAssignment {
    thread: ThreadId,
    row: RowId,
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
                thread.find_row(task.span)
            } else {
                row_id
            }
        } else {
            thread.find_row(task.span)
        };

        thread.rows[row.0].add(task);

        let children = if self.children[task.id.0 as usize].len() > 0 {
            let child_row = thread.find_row(task.span);
            thread.rows[child_row.0].reserve.add(task.span, task.id);
            Some(child_row)
        } else {
            None
        };

        assert!(self.assignments.len() == task.id.0 as usize);
        self.assignments.push(RowAssignment {
            thread: thread_id,
            row,
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
                begin = std::cmp::min(
                    begin,
                    *row.fore.begins.iter().chain(row.back.begins.iter()).min().unwrap());

                end = std::cmp::max(
                    end,
                    *row.fore.ends.iter().chain(row.back.ends.iter()).max().unwrap());
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
