use std::collections::HashMap;

use crate::*;

/// TODO: docs
const RUN_INDICES_ARITY: usize = 32;

/// TODO: docs
#[derive(Clone)]
pub struct RunIndices {
    map: HashMap<ReplicaId, ReplicaIndices>,
}

impl core::fmt::Debug for RunIndices {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        self.map.fmt(f)
    }
}

impl RunIndices {
    #[inline]
    pub fn get_mut(&mut self, id: ReplicaId) -> &mut ReplicaIndices {
        self.map.get_mut(&id).unwrap()
    }

    #[inline]
    pub fn new(id: ReplicaId, idx: LeafIdx<EditRun>, len: u64) -> Self {
        let mut map = HashMap::new();
        map.insert(id, ReplicaIndices::new(idx, len));
        Self { map }
    }
}

/// TODO: docs
#[derive(Clone)]
pub struct ReplicaIndices {
    /// TODO: docs
    insertion_runs: Gtree<RUN_INDICES_ARITY, RunSplits>,

    /// TODO: docs
    run_idxs: Vec<LeafIdx<RunSplits>>,

    /// TODO: docs
    last_run: RunSplit,
}

impl core::fmt::Debug for ReplicaIndices {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        self.insertion_runs.fmt(f)
    }
}

impl ReplicaIndices {
    #[inline]
    pub fn append(&mut self, len: Length, idx: LeafIdx<EditRun>) {
        let new_last = RunSplit::new(len, idx);

        let old_last = core::mem::replace(&mut self.last_run, new_last);

        let (splits, _) = Gtree::new(old_last);

        let (last_idx, _) = self
            .insertion_runs
            .insert(self.insertion_runs.len(), |_, _| (Some(splits), None));

        self.run_idxs.push(last_idx.unwrap());
    }

    #[inline]
    pub fn extend_last(&mut self, extend_by: Length) {
        self.last_run.len += extend_by;
    }

    #[inline]
    pub fn new(first_idx: LeafIdx<EditRun>, len: Length) -> Self {
        let last_run = RunSplit::new(len, first_idx);

        let mut run_idxs = Vec::with_capacity(128);

        Self { insertion_runs: todo!(), run_idxs, last_run }
    }

    #[inline]
    pub fn split(
        &mut self,
        insertion_ts: InsertionTimestamp,
        at_offset: Length,
        right_idx: LeafIdx<EditRun>,
    ) {
        let leaf_idx = self.run_idxs[insertion_ts as usize];

        self.insertion_runs.get_leaf_mut(leaf_idx, |splits| {
            // TODO: at_offset is wrong.
            let (split_idx, split_offset) = splits.leaf_at_offset(at_offset);

            splits.split_leaf(split_idx, |split| {
                split.split(at_offset - split_offset, right_idx)
            });
        });
    }
}

/// TODO: docs
type RunSplits = Gtree<RUN_INDICES_ARITY, RunSplit>;

impl gtree::Join for RunSplits {}

impl gtree::Leaf for RunSplits {
    type Length = Length;
}

impl gtree::Summarize for RunSplits {
    type Summary = Length;

    #[inline]
    fn summarize(&self) -> Self::Summary {
        self.summary()
    }
}

/// TODO: docs
#[derive(Clone, Debug)]
struct RunSplit {
    /// TODO: docs
    len: Length,

    /// TODO: docs
    idx_in_run_tree: LeafIdx<EditRun>,
}

impl RunSplit {
    #[inline]
    fn new(len: Length, idx: LeafIdx<EditRun>) -> Self {
        Self { idx_in_run_tree: idx, len }
    }

    #[inline]
    fn split(&mut self, at_offset: u64, right_idx: LeafIdx<EditRun>) -> Self {
        debug_assert!(at_offset < self.len);
        let right_len = self.len - at_offset;
        self.len = at_offset;
        Self { idx_in_run_tree: right_idx, len: right_len }
    }
}

impl gtree::Summarize for RunSplit {
    type Summary = Length;

    #[inline]
    fn summarize(&self) -> Self::Summary {
        self.len
    }
}

impl gtree::Join for RunSplit {}

impl gtree::Leaf for RunSplit {
    type Length = Length;
}
