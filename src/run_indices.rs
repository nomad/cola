use crate::*;

/// TODO: docs
#[derive(Clone, PartialEq)]
#[cfg_attr(feature = "encode", derive(serde::Serialize, serde::Deserialize))]
pub(crate) struct RunIndices {
    map: ReplicaIdMap<ReplicaIndices>,
}

impl core::fmt::Debug for RunIndices {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        self.map.fmt(f)
    }
}

impl RunIndices {
    /// TODO: docs
    pub fn assert_invariants(&self, run_tree: &RunTree) {
        for (replica_id, indices) in self.iter() {
            for (run_idx, run_offset, run_len) in indices.splits() {
                let run = run_tree.get_run(run_idx);
                assert_eq!(replica_id, run.replica_id());
                assert_eq!(run_offset, run.start());
                assert_eq!(run_len, run.len());
            }
        }
    }

    /// TODO: docs
    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = (ReplicaId, &ReplicaIndices)> {
        self.map.iter().map(|(id, indices)| (*id, indices))
    }

    /// TODO: docs
    #[inline]
    pub fn get_mut(&mut self, id: ReplicaId) -> &mut ReplicaIndices {
        self.map.get_mut(&id).unwrap()
    }

    /// TODO: docs
    #[inline]
    pub fn new(id: ReplicaId, idx: LeafIdx<EditRun>, len: Length) -> Self {
        let mut map = ReplicaIdMap::default();
        map.insert(id, ReplicaIndices::new(idx, len));
        Self { map }
    }
}

/// TODO: docs
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "encode", derive(serde::Serialize, serde::Deserialize))]
pub struct ReplicaIndices {
    /// TODO: docs
    insertion_runs: Gtree<32, InsertionSplits>,

    /// TODO: docs
    run_idxs: Vec<(LeafIdx<InsertionSplits>, Length)>,

    /// TODO: docs
    last_run: InsertionSplits,
}

// impl core::fmt::Debug for ReplicaIndices {
//     fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
//         self.insertion_runs.fmt(f)
//     }
// }

impl ReplicaIndices {
    #[inline]
    pub fn append(&mut self, len: Length, idx: LeafIdx<EditRun>) {
        let last_split = Split::new(len, idx);
        let new_last = InsertionSplits::new(last_split);
        let old_last = core::mem::replace(&mut self.last_run, new_last);
        self.append_split(old_last);
    }

    #[inline]
    fn append_split(&mut self, splits: InsertionSplits) {
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
        self.last_run.extend(extend_by);
    }

    #[inline]
    pub fn move_len_to_next_split(
        &mut self,
        insertion_ts: InsertionTimestamp,
        split_at_offset: Length,
        len_moved: Length,
    ) {
        let idx = insertion_ts as usize;

        let (splits, at_offset) = self.splits_at_offset(idx, split_at_offset);

        splits.move_len_to_next_split(at_offset, len_moved);
    }

    #[inline]
    pub fn move_len_to_prev_split(
        &mut self,
        insertion_ts: InsertionTimestamp,
        split_at_offset: Length,
        len_moved: Length,
    ) {
        let idx = insertion_ts as usize;

        let (splits, at_offset) = self.splits_at_offset(idx, split_at_offset);

        splits.move_len_to_prev_split(at_offset, len_moved);
    }

    #[inline]
    pub fn new(first_idx: LeafIdx<EditRun>, len: Length) -> Self {
        let split = Split::new(len, first_idx);

        Self {
            insertion_runs: Gtree::uninit(),
            run_idxs: Vec::with_capacity(128),
            last_run: InsertionSplits::new(split),
        }
    }

    #[inline]
    pub fn split(
        &mut self,
        insertion_ts: InsertionTimestamp,
        at_offset: Length,
        right_idx: LeafIdx<EditRun>,
    ) {
        let idx = insertion_ts as usize;

        let (splits, at_offset) = self.splits_at_offset(idx, at_offset);

        splits.split(at_offset, right_idx);
    }

    #[inline]
    fn splits_at_offset(
        &mut self,
        idx: usize,
        mut at_offset: Length,
    ) -> (&mut InsertionSplits, Length) {
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

        (splits, at_offset)
    }

    #[inline]
    pub fn splits(&self) -> Splits<'_> {
        let mut run_splits = self.insertion_runs.leaves_from_start();

        let (visited_last, first_split) =
            if let Some(first) = run_splits.next() {
                (false, first)
            } else {
                (true, &self.last_run)
            };

        Splits {
            visited_last,
            current_split: first_split.leaves(),
            last: &self.last_run,
            run_splits,
            offset: 0,
        }
    }
}

/// TODO: docs
const INSERTION_SPLITS_INLINE: usize = 8;

type InsertionSplits = run_splits::InsertionSplits<INSERTION_SPLITS_INLINE>;

type RunSplitLeaves<'a> =
    run_splits::RunSplitLeaves<'a, INSERTION_SPLITS_INLINE>;

mod run_splits {
    use super::*;

    /// TODO: docs
    #[derive(Clone, PartialEq)]
    #[cfg_attr(
        feature = "encode",
        derive(serde::Serialize, serde::Deserialize)
    )]
    pub(super) enum InsertionSplits<const INLINE: usize> {
        /// TODO: docs
        Array(Array<INLINE>),

        /// TODO: docs
        Gtree(Gtree<INLINE, Split>),
    }

    impl<const N: usize> core::fmt::Debug for InsertionSplits<N> {
        fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
            match self {
                Self::Array(array) => array.fmt(f),
                Self::Gtree(gtree) => gtree.fmt(f),
            }
        }
    }

    impl<const INLINE: usize> InsertionSplits<INLINE> {
        #[inline]
        pub fn extend(&mut self, extend_by: Length) {
            match self {
                InsertionSplits::Array(Array { splits, len, total_len }) => {
                    splits[*len - 1].len += extend_by;
                    *total_len += extend_by;
                },

                InsertionSplits::Gtree(splits) => {
                    splits.get_last_leaf_mut(|last| {
                        last.len += extend_by;
                    });
                },
            }
        }

        #[inline]
        pub fn len(&self) -> Length {
            match self {
                Self::Array(array) => array.total_len,
                Self::Gtree(splits) => splits.len(),
            }
        }

        #[inline]
        pub fn move_len_to_next_split(
            &mut self,
            at_offset: Length,
            len_move: Length,
        ) {
            debug_assert!(at_offset < self.len());
            debug_assert!(len_move > 0);

            match self {
                Self::Array(Array { splits, .. }) => {
                    let mut leaf_idx = 0;
                    let mut next_idx = 0;
                    let mut offset = 0;
                    for (idx, split) in splits.iter().enumerate() {
                        offset += split.len;
                        if offset > at_offset {
                            leaf_idx = idx;
                            next_idx = idx + 1;
                            break;
                        }
                    }
                    let (this, next) =
                        crate::get_two_mut(splits, leaf_idx, next_idx);
                    this.len -= len_move;
                    next.len += len_move;
                },

                Self::Gtree(splits) => {
                    let (leaf_idx, _) = splits.leaf_at_offset(at_offset);
                    let next_idx = splits.get_next_leaf(leaf_idx);
                    splits.with_two_mut(leaf_idx, next_idx, |this, next| {
                        this.len -= len_move;
                        next.len += len_move;
                    });
                },
            }
        }

        #[inline]
        pub fn move_len_to_prev_split(
            &mut self,
            at_offset: Length,
            len_move: Length,
        ) {
            debug_assert!(at_offset < self.len());
            debug_assert!(len_move > 0);

            match self {
                Self::Array(Array { splits, .. }) => {
                    let mut prev_idx = 0;
                    let mut leaf_idx = 0;
                    let mut offset = 0;
                    for (idx, split) in splits.iter().enumerate() {
                        offset += split.len;
                        if offset > at_offset {
                            leaf_idx = idx;
                            prev_idx = idx - 1;
                            break;
                        }
                    }
                    let (prev, this) =
                        crate::get_two_mut(splits, prev_idx, leaf_idx);
                    this.len -= len_move;
                    prev.len += len_move;
                },

                Self::Gtree(splits) => {
                    let (leaf_idx, _) = splits.leaf_at_offset(at_offset);
                    let prev_idx = splits.get_prev_leaf(leaf_idx);
                    splits.with_two_mut(prev_idx, leaf_idx, |prev, this| {
                        this.len -= len_move;
                        prev.len += len_move;
                    });
                },
            }
        }

        #[inline]
        pub fn new(first_split: Split) -> Self {
            let mut array = [Split::null(); INLINE];
            let total_len = first_split.len;
            array[0] = first_split;
            Self::Array(Array { splits: array, len: 1, total_len })
        }

        #[inline]
        pub fn split(
            &mut self,
            at_offset: Length,
            right_idx: LeafIdx<EditRun>,
        ) {
            match self {
                InsertionSplits::Array(Array { splits, len, total_len }) => {
                    if *len < INLINE {
                        let mut offset = 0;
                        for (idx, split) in splits.iter_mut().enumerate() {
                            offset += split.len;
                            if offset > at_offset {
                                offset -= split.len;
                                let new_split =
                                    split.split(at_offset - offset, right_idx);
                                crate::insert_in_slice(
                                    splits,
                                    new_split,
                                    idx + 1,
                                );
                                *len += 1;
                                return;
                            }
                        }
                        unreachable!();
                    } else {
                        let gtree = Gtree::from_leaves(
                            splits.iter().copied(),
                            *total_len,
                        );
                        *self = InsertionSplits::Gtree(gtree);
                        self.split(at_offset, right_idx);
                    }
                },

                InsertionSplits::Gtree(splits) => {
                    let (split_idx, split_offset) =
                        splits.leaf_at_offset(at_offset);

                    splits.split_leaf(split_idx, |split| {
                        split.split(at_offset - split_offset, right_idx)
                    });
                },
            };
        }
    }

    #[derive(Clone, PartialEq)]
    pub(super) struct Array<const N: usize> {
        splits: [Split; N],
        len: usize,
        total_len: Length,
    }

    impl<const N: usize> core::fmt::Debug for Array<N> {
        fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
            self.splits().fmt(f)
        }
    }

    impl<const N: usize> Array<N> {
        #[inline]
        fn splits(&self) -> &[Split] {
            &self.splits[..self.len]
        }
    }

    #[cfg(feature = "encode")]
    mod array_serde {
        use serde::ser::SerializeMap;
        use serde::{de, ser};

        use super::*;

        impl<const N: usize> ser::Serialize for Array<N> {
            fn serialize<S: ser::Serializer>(
                &self,
                serializer: S,
            ) -> Result<S::Ok, S::Error> {
                let mut map = serializer.serialize_map(Some(3))?;
                map.serialize_entry("splits", self.splits())?;
                map.serialize_entry("len", &self.len)?;
                map.serialize_entry("total_len", &self.total_len)?;
                map.end()
            }
        }

        impl<'de, const N: usize> de::Deserialize<'de> for Array<N> {
            fn deserialize<D: de::Deserializer<'de>>(
                deserializer: D,
            ) -> Result<Self, D::Error> {
                struct ArrayVisitor<const N: usize>;

                impl<'de, const N: usize> de::Visitor<'de> for ArrayVisitor<N> {
                    type Value = Array<N>;

                    #[inline]
                    fn expecting(
                        &self,
                        formatter: &mut core::fmt::Formatter,
                    ) -> core::fmt::Result {
                        formatter.write_str("a map representing an Array")
                    }

                    #[inline]
                    fn visit_map<V: de::MapAccess<'de>>(
                        self,
                        mut map: V,
                    ) -> Result<Self::Value, V::Error> {
                        let mut len = None;
                        let mut total_len = None;
                        let mut splits_vec = None;

                        while let Some(key) = map.next_key()? {
                            match key {
                                "len" => {
                                    if len.is_some() {
                                        return Err(
                                            de::Error::duplicate_field("len"),
                                        );
                                    }
                                    len = Some(map.next_value()?);
                                },

                                "total_len" => {
                                    if total_len.is_some() {
                                        return Err(
                                            de::Error::duplicate_field(
                                                "total_len",
                                            ),
                                        );
                                    }
                                    total_len = Some(map.next_value()?);
                                },

                                "splits" => {
                                    if splits_vec.is_some() {
                                        return Err(
                                            de::Error::duplicate_field(
                                                "splits",
                                            ),
                                        );
                                    }
                                    splits_vec =
                                        Some(map.next_value::<Vec<Split>>()?);
                                },

                                _ => {
                                    return Err(de::Error::unknown_field(
                                        key,
                                        &["splits", "len", "total_len"],
                                    ));
                                },
                            }
                        }

                        let len = len
                            .ok_or_else(|| de::Error::missing_field("len"))?;

                        let total_len = total_len.ok_or_else(|| {
                            de::Error::missing_field("total_len")
                        })?;

                        let splits_vec = splits_vec.ok_or_else(|| {
                            de::Error::missing_field("splits")
                        })?;

                        if splits_vec.len() != len {
                            return Err(de::Error::invalid_length(
                                splits_vec.len(),
                                &len.to_string().as_str(),
                            ));
                        }

                        if splits_vec.len() > N {
                            return Err(de::Error::invalid_length(
                                splits_vec.len(),
                                &format!("no more than {N}").as_str(),
                            ));
                        }

                        let mut splits = [Split::null(); N];

                        splits[..len].copy_from_slice(splits_vec.as_slice());

                        Ok(Array { splits, len, total_len })
                    }
                }

                deserializer.deserialize_map(ArrayVisitor)
            }
        }
    }

    impl<const N: usize> gtree::Join for InsertionSplits<N> {}

    impl<const N: usize> gtree::Leaf for InsertionSplits<N> {
        type Length = Length;

        #[inline]
        fn len(&self) -> Self::Length {
            self.len()
        }
    }

    impl<const N: usize> InsertionSplits<N> {
        #[inline]
        pub fn leaves(&self) -> RunSplitLeaves<'_, N> {
            match self {
                Self::Array(array) => {
                    RunSplitLeaves::OverArray(array.splits().iter())
                },

                Self::Gtree(gtree) => {
                    RunSplitLeaves::OverGtree(gtree.leaves_from_start())
                },
            }
        }
    }

    pub(super) enum RunSplitLeaves<'a, const N: usize> {
        OverArray(core::slice::Iter<'a, Split>),
        OverGtree(gtree::Leaves<'a, N, Split>),
    }

    impl<'a, const N: usize> Iterator for RunSplitLeaves<'a, N> {
        type Item = &'a Split;

        fn next(&mut self) -> Option<Self::Item> {
            match self {
                Self::OverArray(split) => split.next(),
                Self::OverGtree(splits) => splits.next(),
            }
        }
    }
}

/// TODO: docs
#[derive(Copy, Clone, PartialEq)]
#[cfg_attr(feature = "encode", derive(serde::Serialize, serde::Deserialize))]
struct Split {
    /// TODO: docs
    len: Length,

    /// TODO: docs
    idx_in_run_tree: LeafIdx<EditRun>,
}

impl core::fmt::Debug for Split {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "{} @ {:?}", self.len, self.idx_in_run_tree)
    }
}

impl Split {
    #[inline]
    const fn null() -> Self {
        Self { len: 0, idx_in_run_tree: LeafIdx::dangling() }
    }

    #[inline]
    fn new(len: Length, idx: LeafIdx<EditRun>) -> Self {
        Self { idx_in_run_tree: idx, len }
    }

    #[inline]
    fn split(
        &mut self,
        at_offset: Length,
        right_idx: LeafIdx<EditRun>,
    ) -> Self {
        debug_assert!(at_offset < self.len);
        let right_len = self.len - at_offset;
        self.len = at_offset;
        Self { idx_in_run_tree: right_idx, len: right_len }
    }
}

impl gtree::Join for Split {}

impl gtree::Leaf for Split {
    type Length = Length;

    #[inline]
    fn len(&self) -> Self::Length {
        self.len
    }
}

pub use splits::Splits;

mod splits {
    use super::*;

    pub struct Splits<'a> {
        pub(super) run_splits: gtree::Leaves<'a, 32, InsertionSplits>,
        pub(super) current_split: RunSplitLeaves<'a>,
        pub(super) last: &'a InsertionSplits,
        pub(super) offset: Length,
        pub(super) visited_last: bool,
    }

    impl<'a> Iterator for Splits<'a> {
        type Item = (LeafIdx<EditRun>, Length, Length); // (idx, offset, len)

        fn next(&mut self) -> Option<Self::Item> {
            if let Some(split) = self.current_split.next() {
                let idx = split.idx_in_run_tree;
                let len = split.len;
                let offset = self.offset;
                self.offset += len;
                Some((idx, offset, len))
            } else if let Some(splits) = self.run_splits.next() {
                self.current_split = splits.leaves();
                self.next()
            } else if self.visited_last {
                None
            } else {
                self.visited_last = true;
                self.current_split = self.last.leaves();
                self.next()
            }
        }
    }
}
