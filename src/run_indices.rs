use core::ops::{Index, IndexMut};

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
        for (&replica_id, indices) in self.map.iter() {
            indices.assert_invariants();

            let mut offset = 0;

            for (run_idx, run_len) in indices.splits() {
                let run = run_tree.get_run(run_idx);
                assert_eq!(replica_id, run.replica_id());
                assert_eq!(run_len, run.len());
                assert_eq!(offset, run.start());
                offset += run_len;
            }
        }
    }

    /// TODO: docs
    #[inline]
    pub fn get(&self, id: ReplicaId) -> &ReplicaIndices {
        self.map.get(&id).unwrap()
    }

    /// TODO: docs
    #[inline]
    pub fn get_mut(&mut self, id: ReplicaId) -> &mut ReplicaIndices {
        self.map.entry(id).or_insert_with(ReplicaIndices::new)
    }

    /// TODO: docs
    #[inline]
    pub fn new() -> Self {
        Self { map: ReplicaIdMap::default() }
    }

    /// TODO: docs
    #[inline]
    pub fn update_after_insert(
        &mut self,
        outcome: InsertionOutcome,
        inserted_len: Length,
    ) {
        match outcome {
            InsertionOutcome::ExtendedLastRun { replica_id } => {
                self.get_mut(replica_id).extend_last(inserted_len)
            },

            InsertionOutcome::SplitRun {
                split_id,
                split_insertion,
                split_at_offset,
                split_idx,
                inserted_id,
                inserted_idx,
            } => {
                self.get_mut(inserted_id).append(inserted_len, inserted_idx);

                self.get_mut(split_id).split(
                    split_insertion,
                    split_at_offset,
                    split_idx,
                );
            },

            InsertionOutcome::InsertedRun { replica_id, inserted_idx } => {
                self.get_mut(replica_id).append(inserted_len, inserted_idx)
            },
        };
    }
}

/// TODO: docs
#[derive(Clone, PartialEq)]
#[cfg_attr(feature = "encode", derive(serde::Serialize, serde::Deserialize))]
pub(crate) struct ReplicaIndices {
    vec: Vec<(InsertionSplits, Length)>,
}

impl core::fmt::Debug for ReplicaIndices {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        f.debug_list()
            .entries(self.vec.iter().map(|(splits, _)| splits))
            .finish()
    }
}

/// TODO: docs
impl Index<InsertionTs> for ReplicaIndices {
    type Output = (InsertionSplits, Length);

    #[inline]
    fn index(&self, insertion_ts: InsertionTs) -> &Self::Output {
        &self.vec[insertion_ts as usize]
    }
}

impl IndexMut<InsertionTs> for ReplicaIndices {
    #[inline]
    fn index_mut(&mut self, insertion_ts: InsertionTs) -> &mut Self::Output {
        &mut self.vec[insertion_ts as usize]
    }
}

impl ReplicaIndices {
    #[inline]
    pub fn append(&mut self, len: Length, idx: LeafIdx<EditRun>) {
        let split = Split::new(len, idx);

        let new_last = InsertionSplits::new(split);

        let (last_offset, last_len) = self
            .vec
            .last()
            .map(|(splits, offset)| (splits.len(), *offset))
            .unwrap_or((0, 0));

        self.vec.push((new_last, last_offset + last_len));
    }

    fn assert_invariants(&self) {
        let mut offset = 0;

        for &(ref splits, splits_offset) in self.vec.iter() {
            assert_eq!(splits_offset, offset);
            splits.assert_invariants();
            offset += splits.len();
        }
    }

    #[inline]
    pub fn extend_last(&mut self, extend_by: Length) {
        self.vec.last_mut().unwrap().0.extend(extend_by);
    }

    #[inline]
    pub fn idx_at_offset(
        &self,
        insertion_ts: InsertionTs,
        at_offset: Length,
    ) -> LeafIdx<EditRun> {
        let (splits, offset) = &self[insertion_ts];
        splits.split_at_offset(at_offset - offset).idx_in_run_tree
    }

    #[inline]
    pub fn move_len_to_next_split(
        &mut self,
        insertion_ts: InsertionTs,
        split_at_offset: Length,
        len_moved: Length,
    ) {
        let (splits, offset) = &mut self[insertion_ts];
        splits.move_len_to_next_split(split_at_offset - *offset, len_moved);
    }

    #[inline]
    pub fn move_len_to_prev_split(
        &mut self,
        insertion_ts: InsertionTs,
        split_at_offset: Length,
        len_moved: Length,
    ) {
        let (splits, offset) = &mut self[insertion_ts];
        splits.move_len_to_prev_split(split_at_offset - *offset, len_moved);
    }

    #[inline]
    fn new() -> Self {
        Self { vec: Vec::new() }
    }

    #[inline]
    pub fn split(
        &mut self,
        insertion_ts: InsertionTs,
        at_offset: Length,
        right_idx: LeafIdx<EditRun>,
    ) {
        let (splits, offset) = &mut self[insertion_ts];
        splits.split(at_offset - *offset, right_idx);
    }

    #[inline]
    fn splits(&self) -> impl Iterator<Item = (LeafIdx<EditRun>, Length)> + '_ {
        self.vec.iter().flat_map(|(splits, _)| {
            splits.leaves().map(|split| (split.idx_in_run_tree, split.len))
        })
    }
}

/// TODO: docs
const INSERTION_SPLITS_INLINE: usize = 8;

type InsertionSplits = run_splits::InsertionSplits<INSERTION_SPLITS_INLINE>;

mod run_splits {
    use super::*;

    /// TODO: docs
    #[derive(Clone, PartialEq)]
    #[cfg_attr(
        feature = "encode",
        derive(serde::Serialize, serde::Deserialize)
    )]
    pub(crate) enum InsertionSplits<const INLINE: usize> {
        /// TODO: docs
        Array(Array<INLINE>),

        /// TODO: docs
        Gtree(Gtree<INLINE, Split>),
    }

    impl<const N: usize> core::fmt::Debug for InsertionSplits<N> {
        fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
            match self {
                Self::Array(array) => array.fmt(f),

                Self::Gtree(gtree) => f
                    .debug_list()
                    .entries(gtree.leaves_from_start().map(|(_, split)| split))
                    .finish(),
            }
        }
    }

    impl<const INLINE: usize> InsertionSplits<INLINE> {
        pub fn assert_invariants(&self) {
            match self {
                Self::Array(array) => array.assert_invariants(),
                Self::Gtree(gtree) => gtree.assert_invariants(),
            }
        }

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

        #[inline]
        pub fn split_at_offset(&self, at_offset: Length) -> &Split {
            debug_assert!(at_offset <= self.len());

            match self {
                Self::Array(array) => {
                    let mut offset = 0;
                    for split in array.splits() {
                        offset += split.len;
                        if offset >= at_offset {
                            return split;
                        }
                    }
                    unreachable!();
                },

                Self::Gtree(gtree) => {
                    let (leaf_idx, _) = gtree.leaf_at_offset(at_offset);
                    gtree.get_leaf(leaf_idx)
                },
            }
        }
    }

    #[derive(Clone, PartialEq)]
    pub(crate) struct Array<const N: usize> {
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
        fn assert_invariants(&self) {
            let mut total_len = 0;

            for split in self.splits[..self.len].iter() {
                total_len += split.len;
                assert!(!split.is_null());
            }

            assert_eq!(self.total_len, total_len);

            for split in self.splits[self.len..].iter() {
                assert!(split.is_null());
            }
        }

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
        pub fn leaves(&self) -> RunSplitLeaves<'_> {
            match self {
                Self::Array(array) => {
                    let iter = Box::new(array.splits().iter()) as _;
                    RunSplitLeaves { iter }
                },

                Self::Gtree(gtree) => {
                    let iter = Box::new(
                        gtree.leaves_from_start().map(|(_idx, leaf)| leaf),
                    ) as _;
                    RunSplitLeaves { iter }
                },
            }
        }
    }

    pub(crate) struct RunSplitLeaves<'a> {
        iter: Box<dyn Iterator<Item = &'a Split> + 'a>,
    }

    impl<'a> Iterator for RunSplitLeaves<'a> {
        type Item = &'a Split;

        fn next(&mut self) -> Option<Self::Item> {
            self.iter.next()
        }
    }
}

/// TODO: docs
#[derive(Copy, Clone, PartialEq)]
#[cfg_attr(feature = "encode", derive(serde::Serialize, serde::Deserialize))]
pub(crate) struct Split {
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
    fn is_null(&self) -> bool {
        *self == Self::null()
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
