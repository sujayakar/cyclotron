use crate::util::VecDefaultMap;
use crate::db::{Database, TaskId, Task, Span, NameId};
use crate::layout_algorithm::layout;
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

pub struct Row {
    is_thread: bool,
    pub fore: Chunk,
    pub back: Chunk,
    pub labels: LabelChunk,
}

impl Row {
    pub fn new(is_thread: bool) -> Self {
        Self {
            is_thread,
            fore: Chunk::new(),
            back: Chunk::new(),
            labels: LabelChunk::default(),
        }
    }

    pub fn add(&mut self, task: &Task) {
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
        self.labels.add(task);
    }
}

#[derive(Default)]
pub struct LabelChunk {
    begins: Vec<u64>,
    ends: Vec<u64>,
    names: Vec<NameId>,
}

impl LabelChunk {
    fn add(&mut self, task: &Task) {
        let index = match self.ends.binary_search(&task.span.begin) {
            Ok(index) => index + 1,
            Err(index) => index,
        };
        if index == self.ends.len() {
            self.begins.push(task.span.begin);
            self.ends.push(task.span.end);
            self.names.push(task.name);
        } else {
            assert!(self.begins[index] >= task.span.end);
            self.begins.insert(index, task.span.begin);
            self.ends.insert(index, task.span.end);
            self.names.insert(index, task.name);
        }
    }

    fn iter<'a>(&'a self) -> impl Iterator<Item = (NameId, Span)> + 'a {
        let spans = self.begins.iter().zip(self.ends.iter())
            .map(|(&begin, &end)| Span { begin, end });
        self.names.iter().cloned().zip(spans)
    }

    fn is_empty(&self) -> bool {
        self.begins.is_empty()
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

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct LabelListKey(pub ThreadId, pub RowId);

impl Layout {
    pub fn new(db: &Database, filter: Option<&str>) -> Layout {
        assert!(filter.is_none());
        let mut threads = layout(db);
        let mut tasks_by_name: VecDefaultMap<NameId, usize> = VecDefaultMap::new();
        for task in &db.tasks {
            *tasks_by_name.entry(task.name) += 1;
        }
        let mut table = VecDefaultMap::new();
        let mut group_colors = 1;
        for (name, count) in &tasks_by_name {
            if *count > 0 {
                group_colors += 1;
                *table.entry(name) = GroupId(group_colors);
            }
        }
        for thread in &mut threads {
            for row in &mut thread.rows {
                for chunk in &mut [&mut row.back, &mut row.fore] {
                    for name in &chunk.names {
                        chunk.groups.push(*table.get(*name));
                    }
                }
            }
        }
        Layout { threads }
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

    pub fn iter_labels<'a>(&'a self) -> impl Iterator<Item=(LabelListKey, impl Iterator<Item=(NameId, Span)> + 'a)> + 'a {
        self.threads.iter().enumerate().flat_map(|(tid, t)| {
            t.rows.iter().enumerate().flat_map(move |(rid, r)| {
                if !r.labels.is_empty() {
                    Some((LabelListKey(ThreadId(tid), RowId(rid)), r.labels.iter()))
                } else {
                    None
                }
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
