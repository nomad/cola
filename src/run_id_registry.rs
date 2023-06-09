use core::ops::{Add, AddAssign, Index, IndexMut, Sub, SubAssign};

use crate::node::{Node, Summarize};
use crate::*;

/// TODO: docs
const REPLICA_HISTORIES_ARITY: usize = 4;

/// TODO: docs
const INSERTION_RUNS_ARITY: usize = 4;

/// TODO: docs
#[derive(Clone)]
pub struct RunIdRegistry {
    /// TODO: docs
    histories: Btree<REPLICA_HISTORIES_ARITY, ReplicaHistory>,
}

impl core::fmt::Debug for RunIdRegistry {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        let mut map = f.debug_map();

        map.finish()
    }
}

impl RunIdRegistry {
    /// TODO: docs
    pub fn new(insertion_id: InsertionId, run_id: RunId, len: usize) -> Self {
        debug_assert_eq!(0, insertion_id.local_ts().as_u64());

        let mut history = InsertionHistory::new();

        history.push_insertion(insertion_id.local_ts(), run_id, len);

        let replica_history =
            ReplicaHistory { history, id: insertion_id.replica_id() };

        Self { histories: Btree::from(replica_history) }
    }

    /// TODO: docs
    pub fn get_run_id(&mut self, anchor: &InsertionAnchor) -> Option<RunId> {
        let insertion_id = anchor.insertion_id();

        self.with_history_mut(insertion_id.replica_id(), |history| {
            let insertion = history.get_insertion(insertion_id.local_ts())?;
            Some(insertion.run_at_offset(anchor.offset()))
        })
    }

    /// TODO: docs
    #[inline]
    pub fn push_id(
        &mut self,
        insertion_id: InsertionId,
        insertion_len: usize,
        run_id: RunId,
    ) {
        self.with_history_mut(insertion_id.replica_id(), |history| {
            history.push_insertion(
                insertion_id.local_ts(),
                run_id,
                insertion_len,
            )
        });
    }

    /// TODO: docs
    #[inline]
    pub fn split_id(
        &mut self,
        insertion_id: InsertionId,
        split_at: usize,
        split_run_id: RunId,
    ) {
        self.with_history_mut(insertion_id.replica_id(), |history| {
            history.split_insertion(
                insertion_id.local_ts(),
                split_at,
                split_run_id,
            );
        });
    }

    /// TODO: docs
    fn add_history(&mut self, replica_id: ReplicaId) {
        type Node = node::Node<REPLICA_HISTORIES_ARITY, ReplicaHistory>;

        fn add(node: &mut Node, replica_id: ReplicaId) -> Option<Node> {
            let inode = match node {
                Node::Internal(inode) => inode,

                Node::Leaf(leaf) => {
                    let left_history = if replica_id > leaf.id {
                        ReplicaHistory::new(replica_id)
                    } else {
                        core::mem::replace(
                            leaf,
                            ReplicaHistory::new(replica_id),
                        )
                    };

                    return Some(Node::Leaf(left_history));
                },
            };

            let mut child_idx = None;

            for (idx, child) in inode.children_mut().iter_mut().enumerate() {
                if child.summary().max_replica_id > replica_id {
                    child_idx = Some(idx);
                    break;
                }
            }

            let idx = child_idx.unwrap_or(inode.len() - 1);

            let split =
                inode.with_child_mut(idx, |child| add(child, replica_id));

            split.and_then(|s| inode.insert(idx + 1, s).map(Node::Internal))
        }

        let btree = &mut self.histories;

        if let Some(root_split) = add(btree.root_mut(), replica_id) {
            btree.replace_root(|old_root| {
                Node::from_children(vec![old_root, root_split])
            });
        }
    }

    /// TODO: docs
    fn with_history_mut<F, T>(&mut self, replica_id: ReplicaId, fun: F) -> T
    where
        F: FnOnce(&mut InsertionHistory) -> T,
    {
        let mut node = self.histories.root_mut();

        'outer: loop {
            match node {
                Node::Internal(inode) => {
                    for child in inode.children_mut() {
                        if child.summary().max_replica_id >= replica_id {
                            node = child;
                            continue 'outer;
                        }
                    }

                    self.add_history(replica_id);
                    return self.with_history_mut(replica_id, fun);
                },

                Node::Leaf(ReplicaHistory { id, history }) => {
                    if *id == replica_id {
                        return fun(history);
                    } else {
                        self.add_history(replica_id);
                        return self.with_history_mut(replica_id, fun);
                    }
                },
            };
        }
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

impl ReplicaHistory {
    #[inline]
    fn new(replica_id: ReplicaId) -> Self {
        Self { id: replica_id, history: InsertionHistory::new() }
    }
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
    #[inline]
    fn get_insertion(
        &self,
        insertion_ts: LocalTimestamp,
    ) -> Option<&Insertion> {
        if insertion_ts.as_u64() < self.insertions.len() as u64 {
            Some(&self[insertion_ts])
        } else {
            None
        }
    }

    /// TODO: docs
    ///
    /// # Panics
    ///
    /// Panics if the insertion relative to the previous `LocalTimestamp` was
    /// never `push`ed.
    #[inline]
    fn push_insertion(
        &mut self,
        insertion_ts: LocalTimestamp,
        run_id: RunId,
        insertion_len: usize,
    ) {
        debug_assert_eq!(self.insertions.len() as u64, insertion_ts.as_u64());
        self.insertions.push(Insertion::new(run_id, insertion_len));
    }

    /// TODO: docs
    ///
    /// # Panics
    ///
    /// Panics if the insertion relative to this `LocalTimestamp` was never
    /// [`push`](Self::push)ed.
    #[inline]
    fn split_insertion(
        &mut self,
        insertion_ts: LocalTimestamp,
        split_at: usize,
        left_run_id: RunId,
    ) {
        self[insertion_ts].split(split_at, left_run_id);
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
    fn new(run_id: RunId, len: usize) -> Self {
        Self { runs: Btree::from(InsertionRun::new(run_id, len)) }
    }

    #[inline]
    fn len(&self) -> usize {
        self.runs.summary().len
    }

    /// TODO: docs
    fn run_at_offset(&self, offset: usize) -> RunId {
        debug_assert!(offset <= self.len());

        let mut node = self.runs.root();

        let mut measured = 0;

        'outer: loop {
            match node {
                Node::Internal(inode) => {
                    for child in inode.children() {
                        let child_len = child.summary().len;

                        measured += child_len;

                        if measured >= offset {
                            measured -= child_len;
                            node = child;
                            continue 'outer;
                        }
                    }

                    unreachable!();
                },

                Node::Leaf(InsertionRun { run_id, .. }) => {
                    return run_id.clone()
                },
            }
        }
    }

    /// TODO: docs
    fn split(&mut self, split_at: usize, left_run_id: RunId) {
        type Node = node::Node<INSERTION_RUNS_ARITY, InsertionRun>;

        fn split(
            node: &mut Node,
            split_at: usize,
            left_run_id: RunId,
        ) -> Option<Node> {
            let inode = match node {
                Node::Internal(inode) => inode,

                Node::Leaf(insertion_run) => {
                    return Some(Node::Leaf(
                        insertion_run.split(split_at, left_run_id),
                    ))
                },
            };

            let mut offset = 0;

            for (idx, child) in inode.children_mut().iter_mut().enumerate() {
                let child_len = child.summary().len;

                offset += child_len;

                if offset >= split_at {
                    offset -= child_len;

                    let split = inode.with_child_mut(idx, |child| {
                        split(child, split_at - offset, left_run_id)
                    });

                    return split.and_then(|s| {
                        inode.insert(idx + 1, s).map(Node::Internal)
                    });
                }
            }

            unreachable!();
        }

        debug_assert!(split_at <= self.len());

        if let Some(root_split) =
            split(self.runs.root_mut(), split_at, left_run_id)
        {
            self.runs.replace_root(|old_root| {
                Node::from_children(vec![old_root, root_split])
            });
        };
    }
}

/// TODO: docs
#[derive(Clone, Debug)]
struct InsertionRun {
    /// TODO: docs
    run_id: RunId,

    /// TODO: docs
    len: usize,
}

impl InsertionRun {
    /// TODO: docs
    #[inline]
    fn new(run_id: RunId, len: usize) -> Self {
        Self { run_id, len }
    }

    #[inline]
    fn split(&mut self, at_offset: usize, left_run_id: RunId) -> Self {
        debug_assert!(at_offset < self.len);
        let rest = Self { run_id: left_run_id, len: self.len - at_offset };
        self.len = at_offset;
        rest
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
