use crate::db::{Database, TaskId, Span};
use super::layout::{Thread, Row};
use std::collections::{BTreeSet, HashMap, VecDeque};
use std::ops::Bound;
use std::time::Instant;

// Rectangle for laying out a task and all of its children. A leaf task will have height one, but a
// task with children will have a rectangle that's the bounding box of its rectangle and all of its
// descendents.
#[derive(Clone, Copy, Debug)]
struct LayoutRect {
    time: Span,
    row: u64,
    height: u64,
}

impl LayoutRect {
    fn overlaps(&self, other: &Self) -> bool {
        let left = self.time.end <= other.time.begin;
        let right = other.time.end <= self.time.begin;
        let up = (self.row + self.height) <= other.row;
        let down = (other.row + other.height) <= self.row;
        !(left || right || up || down)
    }
}

// Layout of a single task and its children, where the children have been assigned `row`s so that
// they're nonoverlapping.
#[derive(Debug)]
struct LocalLayout {
    // How tall is the bounding box for this task?
    total_height: u64,

    // height |
    // 0      | [         Parent task         ]
    // 1      | [ First child ] [ Third child ]
    // 2      |        [   Second child   ]
    children: HashMap<TaskId, LayoutRect>,
    children_by_end: BTreeSet<(u64, TaskId)>,
}

impl LocalLayout {
    fn new() -> Self {
        Self {
            total_height: 1,
            children: HashMap::new(),
            children_by_end: BTreeSet::new(),
        }
    }

    // TODO: This is easy to improve algorithmically but Good Enough for now.
    fn add_rect(&mut self, task_id: TaskId, span: Span, height: u64) {
        // To have a rectangle overlap, its end must be greater than our begin.
        let horizontal_overlap = self.children_by_end
            .range((Bound::Excluded((span.begin, task_id)), Bound::Unbounded))
            .map(|(_, task_id)| task_id)
            .collect::<Vec<_>>();

        // Use the smallest `candidate_height` that leads to no overlapping.
        for candidate_height in 1.. {
            let candidate = LayoutRect {
                time: span,
                row: candidate_height,
                height,
            };
            let any_overlap = horizontal_overlap
                .iter()
                .any(|task_id| self.children[task_id].overlaps(&candidate));
            if !any_overlap {
                self.total_height = std::cmp::max(self.total_height, candidate_height + height);
                self.children.insert(task_id, candidate);
                self.children_by_end.insert((candidate.time.end, task_id));
                return;
            }
        }
    }
}

// This layout algorithm operates in two passes. The first inductively lays out a task
// and its children as follows:
//
// 1) For a task with no children, emit a rectangle of height one.
// 2) For a task with children, first compute the layout of its children. Place the task
//    itself at height zero, and then the children in increasing start time order at the
//    lowest height where they don't overlap with any other task. Compute the bounding
//    box of the task and its children and return this rectangle.
//
// The second pass then takes these "local layouts" and then computes a "global" layout
// that matches tasks to rows.
pub fn layout(db: &Database) -> Vec<Thread> {
    let start = Instant::now();

    let mut children_by_task = HashMap::new();
    for task in &db.tasks {
        if let Some(parent) = task.parent {
            children_by_task.entry(parent)
                .or_insert_with(Vec::new)
                .push((task.span.begin, task.id));
        }
    }
    // Ensure children are sorted by start time.
    for children in children_by_task.values_mut() {
        children.sort();
    }

    let mut leaves = VecDeque::new();
    let mut roots = vec![];
    for task in &db.tasks {
        if !children_by_task.contains_key(&task.id) {
            leaves.push_back(task.id);
        }
        if task.parent.is_none() {
            roots.push((task.span.begin, task.id));
        }
    }
    roots.sort();

    // First, start with all of the leaves, which have no children. Process the task tree bottom-up,
    // doing computing layout locally.
    let mut queue = leaves;
    let mut local_layouts: HashMap<_, LocalLayout> = HashMap::new();
    let mut children_remaining: HashMap<_, _> = children_by_task.iter().map(|(k, v)| (k, v.len())).collect();

    while let Some(task_id) = queue.pop_front() {
        let task = db.task(task_id);
        let mut layout = LocalLayout::new();
        if let Some(children) = children_by_task.get(&task_id) {
            for &(_, child) in children {
                let span = db.task(child).span;
                let height = local_layouts[&child].total_height;
                layout.add_rect(child, span, height);
            }
        }
        // If we're the last child to be processed, queue our parent.
        if let Some(parent_id) = task.parent {
            let r = children_remaining.get_mut(&parent_id).unwrap();
            *r -= 1;
            if *r == 0 {
                queue.push_back(parent_id);
            }
        }
        local_layouts.insert(task_id, layout);
    }

    // Okay, now assemble the local layouts into a single global layout.
    let mut threads = vec![];

    for (_, root) in roots.into_iter() {
        let mut thread = Thread { rows: vec![] };
        thread.rows.push(Row::new(true));
        for _ in 1..local_layouts[&root].total_height {
            thread.rows.push(Row::new(false));
        }

        let mut stack = vec![(0, root)];
        while let Some((cur_row, task_id)) = stack.pop() {
            thread.rows[cur_row].add(db.task(task_id));
            let layout = &local_layouts[&task_id];
            if let Some(children) = children_by_task.get(&task_id) {
                for &(_, child_id) in children.iter().rev() {
                    let child_row = cur_row + layout.children[&child_id].row as usize;
                    stack.push((child_row, child_id));
                }
            }
        }
        threads.push(thread);
    }
    println!("Layout for {} tasks took {:?}", db.tasks.len(), start.elapsed());
    threads
}

#[cfg(test)]
mod tests {
    use super::{LayoutRect, layout};
    use crate::db::{Span, NameId, Task, TaskId, Database};

    #[test]
    fn test_layout_rect() {
        let a = LayoutRect {
            time: Span {
                begin: 2,
                end: 10,
            },
            row: 1,
            height: 1,
        };
        let b = LayoutRect {
            time: Span {
                begin: 1,
                end: 17,
            },
            row: 1,
            height: 1,
        };
        assert!(a.overlaps(&b));
    }

    #[test]
    fn test_layout() {
        let tasks = vec![
            Task {
                id: TaskId(0),
                parent: None,
                name: NameId(1),
                span: Span {
                    begin: 0,
                    end: 10,
                },
                on_cpu: None,
            },
            Task {
                id: TaskId(1),
                parent: None,
                name: NameId(1),
                span: Span {
                    begin: 1,
                    end: 12,
                },
                on_cpu: None,
            },
            Task {
                id: TaskId(2),
                parent: Some(TaskId(0)),
                name: NameId(1),
                span: Span {
                    begin: 1,
                    end: 7,
                },
                on_cpu: None,
            },
            Task {
                id: TaskId(3),
                parent: Some(TaskId(0)),
                name: NameId(1),
                span: Span {
                    begin: 2,
                    end: 10,
                },
                on_cpu: None,
            },
            Task {
                id: TaskId(4),
                parent: Some(TaskId(0)),
                name: NameId(1),
                span: Span {
                    begin: 8,
                    end: 9,
                },
                on_cpu: None,
            },
        ];
        let db = Database::test(tasks);
        layout(&db);
    }
}
