use std::collections::HashMap;

use crate::*;

/// TODO: docs
const RUN_INDICES_ARITY: usize = 32;

/// TODO: docs
#[derive(Debug, Clone)]
pub struct RunIndices {
    map: HashMap<ReplicaId, ReplicaRunIndices>,
}

impl RunIndices {
    #[inline]
    pub fn get_mut(&mut self, id: ReplicaId) -> &mut ReplicaRunIndices {
        self.map.get_mut(&id).unwrap()
    }

    #[inline]
    pub fn new(id: ReplicaId, idx: LeafIdx<EditRun>, len: u64) -> Self {
        let mut map = HashMap::new();
        map.insert(id, ReplicaRunIndices::new(idx, len));
        Self { map }
    }
}

/// TODO: docs
#[derive(Clone)]
pub struct ReplicaRunIndices {
    /// TODO: docs
    indices: Gtree<RUN_INDICES_ARITY, EditRunIndex>,

    /// TODO: docs
    last_run_idx: LeafIdx<EditRunIndex>,
}

impl core::fmt::Debug for ReplicaRunIndices {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        self.indices.fmt(f)
    }
}

impl ReplicaRunIndices {
    #[inline]
    pub fn append(&mut self, len: u64, idx: LeafIdx<EditRun>) {
        let run = EditRunIndex::new(idx, len);
        let last_run_idx = self.indices.split_leaf(self.last_run_idx, |_| run);
        self.last_run_idx = last_run_idx;
    }

    #[inline]
    pub fn extend_last(&mut self, extend_by: u64) {
        self.indices.get_leaf_mut(self.last_run_idx, |run| {
            run.len += extend_by;
        });
    }

    #[inline]
    pub fn new(first_idx: LeafIdx<EditRun>, len: u64) -> Self {
        let (indices, last_run_idx) =
            Gtree::new(EditRunIndex::new(first_idx, len));

        Self { indices, last_run_idx }
    }

    #[inline]
    pub fn split(&mut self, at_offset: u64, right_idx: LeafIdx<EditRun>) {
        let (leaf, leaf_offset) = self.indices.leaf_at_offset(at_offset);

        self.indices.split_leaf(leaf, |run| {
            run.split(at_offset - leaf_offset, right_idx)
        });
    }
}

/// TODO: docs
#[derive(Debug, Clone)]
struct EditRunIndex {
    /// TODO: docs
    idx_in_run_tree: LeafIdx<EditRun>,

    /// TODO: docs
    len: u64,
}

impl EditRunIndex {
    #[inline]
    fn new(idx: LeafIdx<EditRun>, run_len: u64) -> Self {
        Self { idx_in_run_tree: idx, len: run_len }
    }

    #[inline]
    fn split(&mut self, at_offset: u64, right_idx: LeafIdx<EditRun>) -> Self {
        debug_assert!(at_offset < self.len);
        let right_len = self.len - at_offset;
        self.len = at_offset;
        Self { idx_in_run_tree: right_idx, len: right_len }
    }
}

impl gtree::Summarize for EditRunIndex {
    type Summary = u64;

    #[inline]
    fn summarize(&self) -> Self::Summary {
        self.len
    }
}

impl gtree::Join for EditRunIndex {}

impl gtree::Leaf for EditRunIndex {
    type Length = u64;
}
