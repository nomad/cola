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
#[derive(Debug, Clone)]
pub struct ReplicaIndices {
    /// TODO: docs
    insertion_runs: Gtree<32, RunSplits>,

    /// TODO: docs
    run_idxs: Vec<(LeafIdx<RunSplits>, Length)>,

    /// TODO: docs
    last_run: RunSplits,
}

// impl core::fmt::Debug for ReplicaIndices {
//     fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
//         self.insertion_runs.fmt(f)
//     }
// }

impl ReplicaIndices {
    #[inline]
    pub fn append(&mut self, len: Length, idx: LeafIdx<EditRun>) {
        let last_split = RunSplit::new(len, idx);
        let new_last = RunSplits::Single(last_split);
        let old_last = core::mem::replace(&mut self.last_run, new_last);
        self.append_split(old_last);
    }

    #[inline]
    fn append_split(&mut self, splits: RunSplits) {
        let last_idx = if self.insertion_runs.is_initialized() {
            self.insertion_runs.append(splits)
        } else {
            self.insertion_runs.initialize(splits)
        };

        let (last_offset, last_len) = self
            .run_idxs
            .last()
            .map(|&(idx, offset)| {
                let splits = self.insertion_runs.get_leaf(idx);
                (offset, splits.len())
            })
            .unwrap_or((0, 0));

        self.run_idxs.push((last_idx, last_offset + last_len));
    }

    #[inline]
    pub fn extend_last(&mut self, extend_by: Length) {
        match &mut self.last_run {
            RunSplits::Single(last) => last.len += extend_by,

            RunSplits::Multi(splits) => {
                splits.get_last_leaf_mut(|last| {
                    last.len += extend_by;
                });
            },
        }
    }

    #[inline]
    pub fn new(first_idx: LeafIdx<EditRun>, len: Length) -> Self {
        let split = RunSplit::new(len, first_idx);

        Self {
            insertion_runs: Gtree::uninit(),
            run_idxs: Vec::with_capacity(128),
            last_run: RunSplits::Single(split),
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

        let splits = if idx == self.run_idxs.len() {
            let offset = self
                .run_idxs
                .last()
                .map(|&(idx, offset)| {
                    let last_split = self.insertion_runs.get_leaf(idx);
                    offset + last_split.len()
                })
                .unwrap_or(0);

            at_offset -= offset;
            &mut self.last_run
        } else {
            let (leaf_idx, run_offset) = self.run_idxs[idx];
            at_offset -= run_offset;
            self.insertion_runs.leaf_mut(leaf_idx)
        };

        match splits {
            RunSplits::Single(split) => {
                let second_split = split.split(at_offset, right_idx);
                let (mut gtree, _) = Gtree::new(split.clone());
                gtree.append(second_split);
                *splits = RunSplits::Multi(gtree);
            },

            RunSplits::Multi(splits) => {
                let (split_idx, split_offset) =
                    splits.leaf_at_offset(at_offset);

                splits.split_leaf(split_idx, |split| {
                    split.split(at_offset - split_offset, right_idx)
                });
            },
        };
    }
}

/// TODO: docs
#[derive(Clone, Debug)]
enum RunSplits {
    Single(RunSplit),
    Multi(Gtree<4, RunSplit>),
}

// impl core::fmt::Debug for RunSplits {
//     fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
//         match self {
//             Self::Single(split) => split.fmt(f),
//             Self::Multi(splits) => splits.fmt(f),
//         }
//     }
// }

impl RunSplits {
    #[inline]
    fn len(&self) -> Length {
        match self {
            Self::Single(split) => split.len,
            Self::Multi(splits) => splits.len(),
        }
    }
}

impl gtree::Join for RunSplits {}

impl gtree::Leaf for RunSplits {
    type Length = Length;
}

impl gtree::Summarize for RunSplits {
    type Summary = Length;

    #[inline]
    fn summarize(&self) -> Self::Summary {
        match self {
            Self::Single(split) => split.len,
            Self::Multi(splits) => splits.summary(),
        }
    }
}

/// TODO: docs
#[derive(Clone)]
struct RunSplit {
    /// TODO: docs
    len: Length,

    /// TODO: docs
    idx_in_run_tree: LeafIdx<EditRun>,
}

impl core::fmt::Debug for RunSplit {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "{} @ {:?}", self.len, self.idx_in_run_tree)
    }
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
