use std::collections::HashMap;

use crate::*;

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
    pub fn iter(&self) -> impl Iterator<Item = (ReplicaId, &ReplicaIndices)> {
        self.map.iter().map(|(id, indices)| (*id, indices))
    }

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
    insertion_runs: Gtree<32, RunSplits>,

    /// TODO: docs
    run_idxs: Vec<(LeafIdx<RunSplits>, Length)>,

    /// TODO: docs
    last_run: Option<RunSplit>,
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

        if let Some(old_last) = self.last_run.replace(new_last) {
            self.append_split(old_last)
        }
    }

    #[inline]
    fn append_split(&mut self, split: RunSplit) {
        let (splits, _) = RunSplits::new(split);

        let last_idx = if self.insertion_runs.is_initialized() {
            self.insertion_runs.append(splits)
        } else {
            self.insertion_runs.initialize(splits)
        };

        let (last_offset, last_len) = self
            .run_idxs
            .last()
            .map(|&(idx, offset)| {
                let len = self.insertion_runs.get_leaf(idx).len();
                (offset, len)
            })
            .unwrap_or((0, 0));

        self.run_idxs.push((last_idx, last_offset + last_len));
    }

    #[inline]
    pub fn extend_last(&mut self, extend_by: Length) {
        if let Some(last) = &mut self.last_run {
            last.len += extend_by;
        } else {
            let &(last_idx, _) = self.run_idxs.last().unwrap();

            self.insertion_runs.get_leaf_mut(last_idx, |splits| {
                let splits_len = splits.len();
                let (last_split_idx, _) = splits.leaf_at_offset(splits_len);
                splits.get_leaf_mut(last_split_idx, |last| {
                    last.len += extend_by;
                });
            });
        }
    }

    #[inline]
    pub fn new(first_idx: LeafIdx<EditRun>, len: Length) -> Self {
        Self {
            insertion_runs: Gtree::uninit(),
            run_idxs: Vec::with_capacity(128),
            last_run: Some(RunSplit::new(len, first_idx)),
        }
    }

    #[inline]
    pub fn split(
        &mut self,
        insertion_ts: InsertionTimestamp,
        mut at_offset: Length,
        right_idx: LeafIdx<EditRun>,
    ) {
        let idx = insertion_ts as usize;

        if idx == self.run_idxs.len() {
            let last = self.last_run.take().unwrap();
            self.append_split(last);
        }

        let (leaf_idx, run_offset) = self.run_idxs[idx];

        at_offset -= run_offset;

        self.insertion_runs.get_leaf_mut(leaf_idx, |splits| {
            let (split_idx, split_offset) = splits.leaf_at_offset(at_offset);

            splits.split_leaf(split_idx, |split| {
                split.split(at_offset - split_offset, right_idx)
            });
        });
    }
}

/// TODO: docs
type RunSplits = Gtree<4, RunSplit>;

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
