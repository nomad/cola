use core::ops::{Add, AddAssign, Index, IndexMut, Sub, SubAssign};

use crate::node::Summarize;
use crate::*;

/// TODO: docs
const REPLICA_HISTORIES_ARITY: usize = 4;

/// TODO: docs
const INSERTION_RUNS_ARITY: usize = 4;

/// TODO: docs
#[derive(Clone, Debug)]
pub struct RunPointers {
    /// TODO: docs
    histories: Btree<REPLICA_HISTORIES_ARITY, ReplicaHistory>,
}

impl RunPointers {
    /// TODO: docs
    pub fn new(first_id: EditId, len: usize) -> Self {
        debug_assert_eq!(0, first_id.local_ts().as_u64());

        let mut history = InsertionHistory::new();

        history.push(first_id.local_ts(), len);

        let replica_history =
            ReplicaHistory { history, id: first_id.replica_id() };

        Self { histories: Btree::from(replica_history) }
    }

    /// TODO: docs
    fn history_mut(&mut self, replica_id: ReplicaId) -> &mut ReplicaHistory {
        todo!();
    }
}

/// TODO: docs
#[derive(Clone, Debug)]
struct ReplicaHistory {
    /// TODO: docs
    id: ReplicaId,

    /// TODO: docs
    history: InsertionHistory,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct MaxReplicaSummary {
    max_replica_id: ReplicaId,
}

impl Default for MaxReplicaSummary {
    #[inline]
    fn default() -> Self {
        Self { max_replica_id: ReplicaId::zero() }
    }
}

impl AddAssign<&Self> for MaxReplicaSummary {
    #[inline]
    fn add_assign(&mut self, other: &Self) {
        if self.max_replica_id < other.max_replica_id {
            *self = *other;
        }
    }
}

impl Add<&Self> for MaxReplicaSummary {
    type Output = Self;

    #[inline]
    fn add(mut self, other: &Self) -> Self::Output {
        self += other;
        self
    }
}

impl SubAssign<&Self> for MaxReplicaSummary {
    #[inline]
    fn sub_assign(&mut self, other: &Self) {
        if self.max_replica_id > other.max_replica_id {
            *self = *other;
        }
    }
}

impl Sub<&Self> for MaxReplicaSummary {
    type Output = Self;

    #[inline]
    fn sub(mut self, other: &Self) -> Self::Output {
        self -= other;
        self
    }
}

impl Summarize for ReplicaHistory {
    type Summary = MaxReplicaSummary;

    #[inline]
    fn summarize(&self) -> Self::Summary {
        MaxReplicaSummary { max_replica_id: self.id }
    }
}

/// TODO: docs
#[derive(Clone, Debug)]
struct InsertionHistory {
    /// TODO: docs
    insertions: Vec<Insertion>,
}

impl InsertionHistory {
    #[inline]
    fn new() -> Self {
        Self { insertions: Vec::new() }
    }

    /// TODO: docs
    ///
    /// # Panics
    ///
    /// Panics if the insertion relative to the previous `LocalTimestamp` was
    /// never `push`ed.
    #[inline]
    fn push(&mut self, insertion_ts: LocalTimestamp, len: usize) {
        debug_assert_eq!(self.insertions.len() as u64, insertion_ts.as_u64());
        self.insertions.push(Insertion::new(len));
    }

    /// TODO: docs
    ///
    /// # Panics
    ///
    /// Panics if the insertion relative to this `LocalTimestamp` was never
    /// [`push`](Self::push)ed.
    #[inline]
    fn split(&mut self, insertion_ts: LocalTimestamp, split_at: usize) {
        self[insertion_ts].split(split_at);
    }
}

/// TODO: docs
impl Index<LocalTimestamp> for InsertionHistory {
    type Output = Insertion;

    #[inline(always)]
    fn index(&self, insertion_ts: LocalTimestamp) -> &Self::Output {
        &self.insertions[insertion_ts.as_u64() as usize]
    }
}

impl IndexMut<LocalTimestamp> for InsertionHistory {
    #[inline(always)]
    fn index_mut(
        &mut self,
        insertion_ts: LocalTimestamp,
    ) -> &mut Self::Output {
        &mut self.insertions[insertion_ts.as_u64() as usize]
    }
}

#[derive(Clone, Debug)]
struct Insertion {
    runs: Btree<INSERTION_RUNS_ARITY, InsertionRun>,
}

impl Insertion {
    /// TODO: docs
    #[inline]
    fn new(len: usize) -> Self {
        Self { runs: Btree::from(InsertionRun::new(len)) }
    }

    #[inline]
    fn len(&self) -> usize {
        self.runs.summary().len
    }

    /// TODO: docs
    fn split(&mut self, split_at: usize) {
        type Node = node::Node<INSERTION_RUNS_ARITY, InsertionRun>;

        fn split(node: &mut Node, split_at: usize) -> Option<Node> {
            let inode = match node {
                Node::Internal(inode) => inode,

                Node::Leaf(insertion_run) => {
                    return insertion_run.split(split_at).map(Node::Leaf)
                },
            };

            let mut offset = 0;

            for (idx, child) in inode.children_mut().iter_mut().enumerate() {
                let child_len = child.summary().len;

                offset += child_len;

                if offset >= split_at {
                    offset -= child_len;

                    let split = inode.with_child_mut(idx, |child| {
                        split(child, split_at - offset)
                    });

                    return split.and_then(|s| {
                        inode.insert(idx + 1, s).map(Node::Internal)
                    });
                }
            }

            unreachable!();
        }

        debug_assert!(split_at <= self.len());

        if let Some(root_split) = split(self.runs.root_mut(), split_at) {
            self.runs.replace_root(|old_root| {
                Node::from_children(vec![old_root, root_split])
            });
        };
    }
}

/// TODO: docs
#[derive(Clone, Copy, Debug)]
struct InsertionRun {
    /// TODO: docs
    len: usize,

    /// TODO: docs
    ptr: *const (),
}

impl InsertionRun {
    /// TODO: docs
    #[inline]
    fn new(len: usize) -> Self {
        Self { len, ptr: &() }
    }

    #[inline]
    fn split(&mut self, at_offset: usize) -> Option<Self> {
        debug_assert!(at_offset <= self.len);

        if at_offset < self.len {
            let mut rest = *self;
            rest.len = self.len - at_offset;
            self.len = at_offset;
            Some(rest)
        } else {
            None
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct RunSummary {
    len: usize,
}

impl AddAssign<&Self> for RunSummary {
    #[inline]
    fn add_assign(&mut self, other: &Self) {
        self.len += other.len;
    }
}

impl Add<&Self> for RunSummary {
    type Output = Self;

    #[inline]
    fn add(mut self, other: &Self) -> Self::Output {
        self += other;
        self
    }
}

impl SubAssign<&Self> for RunSummary {
    #[inline]
    fn sub_assign(&mut self, other: &Self) {
        self.len -= other.len;
    }
}

impl Sub<&Self> for RunSummary {
    type Output = Self;

    #[inline]
    fn sub(mut self, other: &Self) -> Self::Output {
        self -= other;
        self
    }
}

impl Summarize for InsertionRun {
    type Summary = RunSummary;

    #[inline]
    fn summarize(&self) -> Self::Summary {
        RunSummary { len: self.len }
    }
}
