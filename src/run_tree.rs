use core::cmp::Ordering;
use core::ops;

use crate::gtree::LeafIdx;
use crate::*;

/// TODO: docs
const RUN_TREE_ARITY: usize = 32;

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "encode", derive(serde::Serialize, serde::Deserialize))]
pub(crate) struct RunTree {
    /// TODO: docs
    gtree: Gtree<RUN_TREE_ARITY, EditRun>,
}

impl RunTree {
    #[inline]
    fn append_run_to_another(
        &mut self,
        run: EditRun,
        append_to: LeafIdx<EditRun>,
    ) -> (Length, InsertionOutcome) {
        use gtree::Join;

        debug_assert!(self.gtree.get_leaf(append_to).can_append(&run));

        let leaf_len = self.gtree.get_leaf(append_to).len();

        self.gtree.get_leaf_mut(append_to, |leaf| leaf.append(run).unwrap());

        let offset = self.gtree.offset_of_leaf(append_to) + leaf_len;

        (offset, InsertionOutcome::ExtendedLastRun)
    }

    #[inline]
    pub fn assert_invariants(&self) {
        self.gtree.assert_invariants();
    }

    #[inline]
    pub fn average_inode_occupancy(&self) -> f32 {
        self.gtree.average_inode_occupancy()
    }

    #[inline]
    pub fn count_empty_leaves(&self) -> (usize, usize) {
        self.gtree.count_empty_leaves()
    }

    #[inline]
    pub fn debug_as_self(&self) -> DebugAsSelf<'_> {
        self.gtree.debug_as_self()
    }

    #[inline]
    pub fn debug_as_btree(&self) -> DebugAsBtree<'_> {
        self.gtree.debug_as_btree()
    }

    #[inline]
    pub fn delete(
        &mut self,
        range: Range<Length>,
    ) -> (Anchor, Anchor, DeletionOutcome) {
        let mut id_start = ReplicaId::zero();
        let mut insertion_ts_start = 0;
        let mut offset_start = 0;

        let mut id_end = ReplicaId::zero();
        let mut insertion_ts_end = 0;
        let mut offset_end = 0;

        let mut split_across_runs = false;

        let delete_from = |run: &mut EditRun, offset: Length| {
            split_across_runs = true;
            id_start = run.replica_id();
            insertion_ts_start = run.insertion_ts();
            offset_start = run.start() + offset;
            run.delete_from(offset)
        };

        let delete_up_to = |run: &mut EditRun, offset: Length| {
            id_end = run.replica_id();
            insertion_ts_end = run.insertion_ts();
            offset_end = run.start() + offset;
            run.delete_up_to(offset)
        };

        let mut id_range = ReplicaId::zero();
        let mut insertion_ts_range = 0;
        let mut deleted_range_offset = 0;
        let mut deleted_range_run_len = 0;
        let mut deleted_range = Range { start: 0, end: 0 };

        let delete_range = |run: &mut EditRun, range: Range<Length>| {
            id_range = run.replica_id();
            insertion_ts_range = run.insertion_ts();
            deleted_range_offset = run.start();
            deleted_range_run_len = run.len();
            deleted_range = range;
            run.delete_range(range)
        };

        let (first_idx, second_idx) =
            self.gtree.delete(range, delete_range, delete_from, delete_up_to);

        if split_across_runs {
            let anchor_start = Anchor::new(id_start, offset_start);

            let anchor_end = Anchor::new(id_end, offset_end);

            let split_start = first_idx
                .map(|idx| (id_start, insertion_ts_start, offset_start, idx));

            let split_end = second_idx
                .map(|idx| (id_end, insertion_ts_end, offset_end, idx));

            (
                anchor_start,
                anchor_end,
                DeletionOutcome::DeletedAcrossRuns { split_start, split_end },
            )
        } else {
            let anchor_start = Anchor::new(
                id_range,
                deleted_range_offset + deleted_range.start,
            );

            let anchor_end = Anchor::new(
                id_range,
                deleted_range_offset + deleted_range.end,
            );

            let outcome = match (first_idx, second_idx) {
                (Some(first), Some(second)) => {
                    DeletionOutcome::DeletedInMiddleOfSingleRun {
                        replica_id: id_range,
                        insertion_ts: insertion_ts_range,
                        range: deleted_range + deleted_range_offset,
                        idx_of_deleted: first,
                        idx_of_split: second,
                    }
                },

                (Some(first), _) => DeletionOutcome::DeletionSplitSingleRun {
                    replica_id: id_range,
                    insertion_ts: insertion_ts_range,
                    offset: deleted_range_offset
                        + if deleted_range.start == 0 {
                            deleted_range.end
                        } else {
                            deleted_range.start
                        },
                    idx: first,
                },

                (None, None) => {
                    if deleted_range.len() == deleted_range_run_len {
                        DeletionOutcome::DeletedWholeRun
                    } else if deleted_range.start == 0 {
                        DeletionOutcome::DeletionMergedInPreviousRun {
                            replica_id: id_range,
                            insertion_ts: insertion_ts_range,
                            offset: deleted_range_offset + deleted_range.end,
                            deleted: deleted_range.len(),
                        }
                    } else if deleted_range.end == deleted_range_run_len {
                        DeletionOutcome::DeletionMergedInNextRun {
                            replica_id: id_range,
                            insertion_ts: insertion_ts_range,
                            offset: deleted_range_offset + deleted_range.start,
                            deleted: deleted_range.len(),
                        }
                    } else {
                        unreachable!();
                    }
                },

                _ => unreachable!(),
            };

            (anchor_start, anchor_end, outcome)
        }
    }

    #[inline]
    pub fn get_run(&self, run_idx: LeafIdx<EditRun>) -> &EditRun {
        self.gtree.get_leaf(run_idx)
    }

    #[inline]
    pub fn insert(
        &mut self,
        offset: Length,
        text: Text,
        insertion_clock: &mut InsertionClock,
        lamport_clock: &mut LamportClock,
    ) -> (Anchor, InsertionTimestamp, InsertionOutcome) {
        debug_assert!(!text.range.is_empty());

        let mut anchor_ts = 0;

        if offset == 0 {
            let run = EditRun::new(
                Anchor::origin(),
                text,
                insertion_clock.next(),
                lamport_clock.next(),
            );

            let inserted_idx = self.gtree.prepend(run);

            let outcome = InsertionOutcome::InsertedRun { inserted_idx };

            return (Anchor::origin(), anchor_ts, outcome);
        }

        let mut split_id = ReplicaId::zero();

        let mut split_insertion = 0;

        let mut split_at_offset = 0;

        let mut anchor = Anchor::origin();

        let insert_with = |run: &mut EditRun, offset: Length| {
            split_id = run.replica_id();
            split_insertion = run.insertion_ts();
            split_at_offset = run.start() + offset;
            anchor_ts = run.insertion_ts();
            anchor = Anchor::new(run.replica_id(), run.end());

            if run.len() == offset
                && run.replica_id() == text.inserted_by()
                && run.end() == text.range.start
            {
                run.extend(text.range.len());
                return (None, None);
            }

            let split = run.split(offset);

            let new_run = EditRun::new(
                anchor,
                text,
                insertion_clock.next(),
                lamport_clock.next(),
            );

            (Some(new_run), split)
        };

        let (inserted_idx, split_idx) = self.gtree.insert(offset, insert_with);

        let outcome = match (inserted_idx, split_idx) {
            (None, None) => InsertionOutcome::ExtendedLastRun,

            (Some(inserted_idx), Some(split_idx)) => {
                InsertionOutcome::SplitRun {
                    split_id,
                    split_insertion,
                    split_at_offset,
                    split_idx,
                    inserted_idx,
                }
            },

            (Some(inserted_idx), None) => {
                InsertionOutcome::InsertedRun { inserted_idx }
            },

            _ => unreachable!(),
        };

        (anchor, anchor_ts, outcome)
    }

    #[inline]
    fn insert_run_after_another(
        &mut self,
        run: EditRun,
        insert_after: LeafIdx<EditRun>,
    ) -> (Length, InsertionOutcome) {
        let inserted_idx =
            self.gtree.insert_leaf_after_another(run, insert_after);

        let offset = self.gtree.offset_of_leaf(inserted_idx);

        (offset, InsertionOutcome::InsertedRun { inserted_idx })
    }

    #[inline]
    fn insert_run_at_origin(
        &mut self,
        run: EditRun,
    ) -> (Length, InsertionOutcome) {
        debug_assert!(run.anchor().is_origin());

        let mut leaves = self.gtree.leaves_from_start();

        let (mut prev_idx, first_leaf) = leaves.next().unwrap();

        if run.partial_cmp(first_leaf) != Some(Ordering::Greater) {
            let inserted_idx = self.gtree.prepend(run);
            return (0, InsertionOutcome::InsertedRun { inserted_idx });
        }

        for (idx, leaf) in leaves {
            if &run > leaf {
                prev_idx = idx;
            } else {
                return self.insert_run_after_another(run, prev_idx);
            }
        }

        // If we get here we're inserting after the last run in the Gtree.
        self.insert_run_after_another(run, prev_idx)
    }

    #[inline]
    pub fn len(&self) -> Length {
        self.gtree.len()
    }

    #[inline]
    pub fn merge_deletion(&self, deletion: &Deletion) -> MergedDeletion {
        todo!();
    }

    #[inline]
    pub fn merge_insertion(
        &mut self,
        insertion: &Insertion,
        run_indices: &RunIndices,
    ) -> (Length, InsertionOutcome) {
        let run = EditRun::from_insertion(insertion);

        if run.anchor().is_origin() {
            return self.insert_run_at_origin(run);
        }

        // Get the leaf index of the EditRun that contains the anchor of the
        // Insertion.
        let anchor_idx = run_indices
            .get(run.anchor().replica_id())
            .idx_at_offset(insertion.anchor_ts(), insertion.anchor().offset);

        let anchor = self.gtree.get_leaf(anchor_idx);

        // If the insertion is anchored in the middle of the anchor run then
        // there can't be any other runs that are tied with it. In this case we
        // can just split the anchor run and insert the new run after it.
        if run.anchor().offset < anchor.end() {
            let insert_at = anchor.end() - run.anchor().offset;
            return self.split_run_with_another(run, anchor_idx, insert_at);
        }

        let mut prev_idx = anchor_idx;

        // Before creating the `Leaves` iterator (which would allocate) we
        // check if we can rule out any possible ties by only using the direct
        // siblings of the anchor run.
        let mut siblings = self.gtree.siblings::<false>(anchor_idx);

        if let Some((idx, next_sibling)) = siblings.next() {
            // The next sibling is tied with the run we're inserting -> check
            // the other siblings.
            if &run > next_sibling {
                prev_idx = idx;

                for (idx, sibling) in siblings {
                    if &run > sibling {
                        prev_idx = idx;
                    } else {
                        return self.insert_run_after_another(run, prev_idx);
                    }
                }
            } else if anchor.can_append(&run) {
                // Append the run to the anchor run. This is the only path that
                // doesn't add new runs to the Gtree.
                return self.append_run_to_another(run, anchor_idx);
            } else {
                // Insert the run right after the anchor run.
                return self.insert_run_after_another(run, anchor_idx);
            }
        };

        for (idx, leaf) in self.gtree.leaves::<false>(prev_idx) {
            if &run > leaf {
                prev_idx = idx;
            } else {
                return self.insert_run_after_another(run, prev_idx);
            }
        }

        // If we get here we're inserting after the last run in the Gtree.
        self.insert_run_after_another(run, prev_idx)
    }

    #[inline]
    pub fn new(first_run: EditRun) -> (Self, LeafIdx<EditRun>) {
        let (gtree, idx) = Gtree::new(first_run);
        (Self { gtree }, idx)
    }

    #[inline]
    fn split_run_with_another(
        &mut self,
        run: EditRun,
        insert_into: LeafIdx<EditRun>,
        at_offset: Length,
    ) -> (Length, InsertionOutcome) {
        todo!();
    }
}

/// TODO: docs
#[allow(clippy::enum_variant_names)]
pub(crate) enum InsertionOutcome {
    /// TODO: docs
    ExtendedLastRun,

    /// TODO: docs
    InsertedRun { inserted_idx: LeafIdx<EditRun> },

    /// TODO: docs
    SplitRun {
        split_id: ReplicaId,
        split_insertion: InsertionTimestamp,
        split_at_offset: Length,
        split_idx: LeafIdx<EditRun>,
        inserted_idx: LeafIdx<EditRun>,
    },
}

/// TODO: docs
pub(crate) enum DeletionOutcome {
    /// TODO: docs
    DeletedAcrossRuns {
        split_start:
            Option<(ReplicaId, InsertionTimestamp, Length, LeafIdx<EditRun>)>,

        split_end:
            Option<(ReplicaId, InsertionTimestamp, Length, LeafIdx<EditRun>)>,
    },

    /// TODO: docs
    DeletedInMiddleOfSingleRun {
        replica_id: ReplicaId,
        insertion_ts: InsertionTimestamp,
        range: Range<Length>,
        idx_of_deleted: LeafIdx<EditRun>,
        idx_of_split: LeafIdx<EditRun>,
    },

    /// TODO: docs
    DeletionSplitSingleRun {
        replica_id: ReplicaId,
        insertion_ts: InsertionTimestamp,
        offset: Length,
        idx: LeafIdx<EditRun>,
    },

    /// TODO: docs
    DeletionMergedInPreviousRun {
        replica_id: ReplicaId,
        insertion_ts: InsertionTimestamp,
        offset: Length,
        deleted: Length,
    },

    /// TODO: docs
    DeletionMergedInNextRun {
        replica_id: ReplicaId,
        insertion_ts: InsertionTimestamp,
        offset: Length,
        deleted: Length,
    },

    DeletedWholeRun,
}

/// TODO: docs
#[derive(Debug)]
pub(crate) enum MergedDeletion {
    Contiguous(ops::Range<Length>),
    Split(Vec<ops::Range<Length>>),
}

/// TODO: docs
#[derive(Clone, PartialEq)]
#[cfg_attr(feature = "encode", derive(serde::Serialize, serde::Deserialize))]
pub(crate) struct EditRun {
    /// TODO: docs
    inserted_at: Anchor,

    /// TODO: docs
    text: Text,

    /// TODO: docs
    insertion_ts: InsertionTimestamp,

    /// TODO: docs
    lamport_ts: LamportTimestamp,

    /// TODO: docs
    is_deleted: bool,
}

impl core::fmt::Debug for EditRun {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(
            f,
            "{:?} |@ {:?} - L({}) I({}){}",
            self.text,
            self.inserted_at,
            self.lamport_ts,
            self.insertion_ts,
            if self.is_deleted { " 🪦" } else { "" },
        )
    }
}

/// This implementation is guaranteed to never return `Some(Ordering::Equal)`.
impl PartialOrd for EditRun {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        // If the two runs were inserted at different positions they're totally
        // unrelated and we can't compare them.
        if self.inserted_at != other.inserted_at {
            return None;
        };

        // If they have the same anchor we first sort descending on lamport
        // timestamsps, and if those are also the same we use the replica id as
        // a last tie breaker (here we sort ascending on replica ids but that's
        // totally arbitrary).
        Some(match other.lamport_ts.cmp(&self.lamport_ts) {
            Ordering::Equal => self.replica_id().cmp(&other.replica_id()),
            other => other,
        })
    }
}

impl EditRun {
    #[inline(always)]
    pub fn anchor(&self) -> &Anchor {
        &self.inserted_at
    }

    #[inline]
    pub fn can_append(&self, other: &Self) -> bool {
        self.is_deleted == other.is_deleted
            && self.replica_id() == other.replica_id()
            && self.end() == other.start()
    }

    #[inline]
    pub fn can_prepend(&self, other: &Self) -> bool {
        self.is_deleted == other.is_deleted
            && self.replica_id() == other.replica_id()
            && other.end() == self.start()
    }

    #[inline(always)]
    pub fn end(&self) -> Length {
        self.text.range.end
    }

    #[inline(always)]
    fn end_mut(&mut self) -> &mut Length {
        &mut self.text.range.end
    }

    #[inline(always)]
    pub fn extend(&mut self, extend_by: Length) {
        self.text.range.end += extend_by;
    }

    #[inline]
    fn delete_from(&mut self, offset: Length) -> Option<Self> {
        if offset == 0 {
            self.is_deleted = true;
            None
        } else if offset < self.len() {
            let mut del = self.split(offset)?;
            del.is_deleted = true;
            Some(del)
        } else {
            None
        }
    }

    #[inline]
    fn delete_range(
        &mut self,
        Range { start, end }: Range<Length>,
    ) -> (Option<Self>, Option<Self>) {
        debug_assert!(start <= end);

        if start == end {
            (None, None)
        } else if start == 0 {
            (self.delete_up_to(end), None)
        } else if end >= self.len() {
            (self.delete_from(start), None)
        } else {
            let rest = self.split(end);
            let deleted = self.split(start).map(|mut d| {
                d.is_deleted = true;
                d
            });
            (deleted, rest)
        }
    }

    #[inline]
    fn delete_up_to(&mut self, offset: Length) -> Option<Self> {
        if offset == 0 {
            None
        } else if offset < self.len() {
            let rest = self.split(offset);
            self.is_deleted = true;
            rest
        } else {
            self.is_deleted = true;
            None
        }
    }

    #[inline(always)]
    pub fn from_insertion(insertion: &Insertion) -> Self {
        Self {
            inserted_at: insertion.anchor().clone(),
            text: insertion.text().clone(),
            insertion_ts: insertion.insertion_ts(),
            lamport_ts: insertion.lamport_ts(),
            is_deleted: false,
        }
    }

    #[inline(always)]
    pub fn insertion_ts(&self) -> InsertionTimestamp {
        self.insertion_ts
    }

    /// TODO: docs
    #[inline]
    pub fn len(&self) -> Length {
        self.end() - self.start()
    }

    /// TODO: docs
    #[inline]
    pub fn new(
        inserted_at: Anchor,
        text: Text,
        insertion_ts: InsertionTimestamp,
        lamport_ts: LamportTimestamp,
    ) -> Self {
        Self { inserted_at, text, insertion_ts, lamport_ts, is_deleted: false }
    }

    #[inline(always)]
    pub fn replica_id(&self) -> ReplicaId {
        self.text.inserted_by()
    }

    /// TODO: docs
    #[inline(always)]
    pub fn split(&mut self, at_offset: Length) -> Option<Self> {
        if at_offset == self.len() || at_offset == 0 {
            None
        } else {
            let mut split = self.clone();
            split.text.range.start += at_offset;
            self.text.range.end = split.text.range.start;
            Some(split)
        }
    }

    #[inline(always)]
    pub fn start(&self) -> Length {
        self.text.range.start
    }

    #[inline(always)]
    fn start_mut(&mut self) -> &mut Length {
        &mut self.text.range.start
    }
}

/// TODO: docs
#[derive(Copy, Clone, PartialEq)]
#[cfg_attr(
    any(feature = "encode", feature = "serde"),
    derive(serde::Serialize, serde::Deserialize)
)]
pub struct Anchor {
    /// TODO: docs
    replica_id: ReplicaId,

    /// TODO: docs
    offset: Length,
}

impl core::fmt::Debug for Anchor {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        if self == &Self::origin() {
            write!(f, "origin")
        } else {
            write!(f, "{:x}.{}", self.replica_id.as_u32(), self.offset)
        }
    }
}

impl Anchor {
    #[inline]
    pub fn character_ts(&self) -> Length {
        self.offset
    }

    #[inline(always)]
    pub fn is_origin(&self) -> bool {
        self == &Self::origin()
    }

    #[inline(always)]
    pub fn new(replica_id: ReplicaId, offset: Length) -> Self {
        Self { replica_id, offset }
    }

    /// A special value used to create an anchor at the start of the document.
    #[inline]
    pub const fn origin() -> Self {
        Self { replica_id: ReplicaId::zero(), offset: 0 }
    }

    #[inline]
    pub fn replica_id(&self) -> ReplicaId {
        self.replica_id
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Diff {
    Add(Length),
    Subtract(Length),
}

impl gtree::Length for Length {
    type Diff = Diff;

    #[inline]
    fn zero() -> Self {
        0
    }

    #[inline]
    fn diff(from: Self, to: Self) -> Diff {
        if from < to {
            Diff::Add(to - from)
        } else {
            Diff::Subtract(from - to)
        }
    }

    #[inline]
    fn apply_diff(&mut self, diff: Diff) {
        match diff {
            Diff::Add(add) => *self += add,
            Diff::Subtract(sub) => *self -= sub,
        }
    }
}

impl gtree::Join for EditRun {
    #[inline]
    fn append(&mut self, other: Self) -> Result<(), Self> {
        if self.can_append(&other) {
            *self.end_mut() = other.end();
            Ok(())
        } else {
            Err(other)
        }
    }

    #[inline]
    fn prepend(&mut self, other: Self) -> Result<(), Self> {
        if self.can_prepend(&other) {
            debug_assert_eq!(self.insertion_ts, other.insertion_ts);
            *self.start_mut() = other.start();
            Ok(())
        } else {
            Err(other)
        }
    }
}

impl gtree::Delete for EditRun {
    fn delete(&mut self) {
        self.is_deleted = true;
    }
}

impl gtree::Leaf for EditRun {
    type Length = Length;

    #[inline]
    fn len(&self) -> Self::Length {
        self.len() * (!self.is_deleted as Length)
    }
}

pub(crate) type DebugAsBtree<'a> =
    gtree::DebugAsBtree<'a, RUN_TREE_ARITY, EditRun>;

pub(crate) type DebugAsSelf<'a> =
    gtree::DebugAsSelf<'a, RUN_TREE_ARITY, EditRun>;
