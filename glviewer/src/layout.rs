use crate::util::VecDefaultMap;
use std::collections::HashMap;
use bit_set::BitSet;
use crate::db::{Database, TaskId, Task, Span, NameId};
use std::collections::HashSet;
use std::time::Duration;

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
    pub groups: Vec<GroupId>,
    pub tasks: Vec<TaskId>,
}

impl Chunk {
    fn new() -> Chunk {
        Chunk {
            begins: Vec::new(),
            ends: Vec::new(),
            names: Vec::new(),
            groups: Vec::new(),
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
            groups: &self.groups,
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
        groups: vec![],
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
    groups: &'a [GroupId],
}

impl<'a> Iterator for ChunkSpanIter<'a> {
    type Item = (GroupId, NameId, Span);
    fn next(&mut self) -> Option<(GroupId, NameId, Span)> {
        if self.begins.len() > 0 {
            let res = (
                self.groups[0],
                self.names[0],
                Span { begin: self.begins[0], end: self.ends[0] },
            );
            self.begins = &self.begins[1..];
            self.ends = &self.ends[1..];
            self.names = &self.names[1..];
            self.groups = &self.groups[1..];
            Some(res)
        } else {
            None
        }
    }
}

#[derive(Default, Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct GroupId(pub u32);

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
    tasks_by_name: VecDefaultMap<NameId, usize>,
    children: HashMap<TaskId, Vec<TaskId>>,
    task_to_thread: HashMap<TaskId, ThreadId>,
    threads: Vec<Thread>,
    assignments: HashMap<TaskId, RowAssignment>,
}

impl LayoutBuilder {
    fn add(&mut self, task: &Task) {
        let thread_id = if let Some(parent) = task.parent {
            self.task_to_thread[&parent]
        } else {
            let thread_id = ThreadId(self.threads.len());
            self.threads.push(Thread {
                rows: Vec::new(),
            });
            thread_id
        };

        *self.tasks_by_name.entry(task.name) += 1;

        self.task_to_thread.insert(task.id, thread_id);

        let thread = &mut self.threads[thread_id.0];

        let row = if let Some(parent) = task.parent {
            let row_id = self.assignments[&parent].children.unwrap();
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

        let children = if self.children.contains_key(&task.id) {
            let child_row = thread.find_row(task.span, task.parent.is_none());
            thread.rows[child_row.0].reserve.add(task.span, task.name, task.id);
            Some(child_row)
        } else {
            None
        };

        self.assignments.insert(task.id, RowAssignment {
            // thread: thread_id,
            // row,
            children,
        });
    }

    fn compute_task_name_table(&self) -> VecDefaultMap<NameId, GroupId> {
        let mut res = VecDefaultMap::new();

        let mut group_colors = 1;

        for (name, count) in &self.tasks_by_name {
            if *count > 0 {
                group_colors += 1;
                *res.entry(name) = GroupId(group_colors);
            }
        }

        res
    }

    fn fill_group_names(&mut self) {
        let table = self.compute_task_name_table();

        for thread in &mut self.threads {
            for row in &mut thread.rows {
                for chunk in &mut [&mut row.back, &mut row.fore] {
                    for name in &chunk.names {
                        chunk.groups.push(*table.get(*name));
                    }
                }
            }
        }
    }
}

impl Layout {
    pub fn new(db: &Database, filter: Option<&str>) -> Layout {
        let mut filtered_names = BitSet::new();

        if let Some(filter) = filter {
            for (id, name) in db.names().enumerate() {
                if name.find(filter).is_some() {
                    filtered_names.insert(id);
                }
            }
        }

        let mut children = HashMap::new();
        for task in &db.tasks {
            if let Some(parent) = task.parent {
                children.entry(parent).or_insert_with(Vec::new).push(task.id);
            }
        }

        let mut b = LayoutBuilder {
            tasks_by_name: VecDefaultMap::new(),
            children,
            task_to_thread: HashMap::new(),
            threads: Vec::new(),
            assignments: HashMap::new(),
        };

        if filter.is_some() {
            let mut parents = Vec::new();
            let mut parent_tasks = HashSet::new();

            for task in &db.tasks {
                if filtered_names.contains(task.name.0 as usize) {
                    if let Some(parent) = task.parent {
                        parents.push(parent);
                        parent_tasks.insert(parent);
                    }
                }
            }

            while let Some(parent) = parents.pop() {
                let task = &db.tasks[parent.0 as usize];

                if let Some(parent) = task.parent {
                    parents.push(parent);
                    parent_tasks.insert(parent);
                }
            }

            for task in &db.tasks {
                if parent_tasks.contains(&task.id) || filtered_names.contains(task.name.0 as usize) {
                    b.add(task)
                }
            }
        } else {
            for task in &db.tasks {
                b.add(task)
            }
        }

        b.fill_group_names();

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

    pub fn span_count(&self) -> usize {
        let mut sum = 0;

        for t in &self.threads {
            for row in &t.rows {
                sum += row.fore.begins.len();
                sum += row.back.begins.len();
            }
        }
        sum
    }

    pub fn print_all_spans(&self, db: &Database) {
        for t in &self.threads {
            for row in &t.rows {
                for chunk in &[&row.fore, &row.back] {
                    for (name, (begin, end)) in chunk.names.iter().zip(chunk.begins.iter().zip(&chunk.ends)) {
                        println!("  start {:?} length {:?} : {}",
                            Duration::from_nanos(*begin),
                            Duration::from_nanos(end - begin),
                            db.name(*name));
                    }
                }
            }
        }
    }
}
