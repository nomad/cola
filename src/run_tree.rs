use core::cmp::Ordering;

use crate::gtree::LeafIdx;
use crate::*;

/// TODO: docs
const RUN_TREE_ARITY: usize = 32;

#[derive(Clone, Debug)]
pub struct RunTree {
    /// TODO: docs
    pub gtree: Gtree<RUN_TREE_ARITY, EditRun>,

    /// TODO: docs
    this_id: ReplicaId,
}

impl RunTree {
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
    pub fn delete(&mut self, range: Range<Length>) -> DeletionOutcome {
        let mut id_start = ReplicaId::zero();
        let mut offset_start = 0;

        let mut id_end = ReplicaId::zero();
        let mut offset_end = 0;

        let mut split_across_runs = false;

        let delete_from = |run: &mut EditRun, offset: Length| {
            split_across_runs = true;
            id_start = run.replica_id();
            offset_start = run.start() + offset;
            run.delete_from(offset)
        };

        let delete_up_to = |run: &mut EditRun, offset: Length| {
            id_end = run.replica_id();
            offset_end = run.start() + offset;
            run.delete_up_to(offset)
        };

        let mut id_range = ReplicaId::zero();
        let mut deleted_range = Range { start: 0, end: 0 };
        let mut deleted_left_part = 0;
        let mut deleted_right_part = 0;

        let delete_range = |run: &mut EditRun, range: Range<Length>| {
            id_range = run.replica_id();
            if range.start == 0 {
                deleted_left_part = run.start() + range.end;
            } else if range.end == run.len() {
                deleted_right_part = run.start() + range.start;
            } else {
                deleted_range.start = run.start() + range.start;
                deleted_range.end = run.start() + range.end;
            }
            run.delete_range(range)
        };

        let (first_idx, second_idx) =
            self.gtree.delete(range, delete_range, delete_from, delete_up_to);

        if split_across_runs {
            let split_start =
                first_idx.map(|idx| (id_start, offset_start, idx));

            let split_end = second_idx.map(|idx| (id_end, offset_end, idx));

            DeletionOutcome::DeletedAcrossRuns { split_start, split_end }
        } else {
            match (first_idx, second_idx) {
                (Some(first), Some(second)) => {
                    DeletionOutcome::DeletedInMiddleOfSingleRun {
                        replica_id: id_range,
                        range: deleted_range,
                        idx_of_deleted: first,
                        idx_of_split: second,
                    }
                },

                (Some(first), _) => DeletionOutcome::DeletionSplitSingleRun {
                    replica_id: id_range,
                    offset: core::cmp::max(
                        deleted_left_part,
                        deleted_right_part,
                    ),
                    idx: first,
                },

                _ => DeletionOutcome::Wip,
            }
        }
    }

    #[inline]
    pub fn insert(
        &mut self,
        offset: Length,
        run_len: Length,
        character_ts: Length,
        insertion_clock: &mut InsertionClock,
        lamport_clock: &mut LamportClock,
    ) -> (Anchor, InsertionOutcome) {
        debug_assert!(run_len > 0);

        let mut split_id = self.this_id;

        let mut split_at_offset = 0;

        let mut anchor = Anchor::origin();

        let insert_with = |run: &mut EditRun, offset: u64| {
            split_id = run.replica_id();
            split_at_offset = run.start() + offset;

            if run.len() == offset
                && run.replica_id() == self.this_id
                && run.end() == character_ts
            {
                anchor = Anchor::new(run.replica_id(), run.end());
                run.extend(run_len);
                return (None, None);
            }

            let range = (character_ts..character_ts + run_len).into();

            let insertion_ts = insertion_clock.next();

            let lamport_ts = lamport_clock.next();

            if offset == 0 {
                let new_run = EditRun::new(
                    anchor.clone(),
                    self.this_id,
                    range,
                    insertion_ts,
                    lamport_ts,
                );

                let this_run = core::mem::replace(run, new_run);

                (Some(this_run), None)
            } else {
                let split = run.split(offset);

                anchor = Anchor::new(run.replica_id(), run.end());

                let new_run = EditRun::new(
                    anchor.clone(),
                    self.this_id,
                    range,
                    insertion_ts,
                    lamport_ts,
                );

                (Some(new_run), split)
            }
        };

        let (inserted_idx, split_idx) = self.gtree.insert(offset, insert_with);

        let outcome = match (inserted_idx, split_idx) {
            (None, None) => InsertionOutcome::ExtendedLastRun,

            (Some(inserted_idx), Some(split_idx)) => {
                InsertionOutcome::SplitRun {
                    split_id,
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

        (anchor, outcome)
    }

    #[inline]
    pub fn len(&self) -> u64 {
        self.gtree.summary()
    }

    #[inline]
    pub fn new(
        this_id: ReplicaId,
        first_run: EditRun,
    ) -> (Self, LeafIdx<EditRun>) {
        let (gtree, idx) = Gtree::new(first_run);
        (Self { this_id, gtree }, idx)
    }
}

/// TODO: docs
pub enum InsertionOutcome {
    /// TODO: docs
    ExtendedLastRun,

    /// TODO: docs
    InsertedRun { inserted_idx: LeafIdx<EditRun> },

    /// TODO: docs
    SplitRun {
        split_id: ReplicaId,
        split_at_offset: Length,
        split_idx: LeafIdx<EditRun>,
        inserted_idx: LeafIdx<EditRun>,
    },
}

/// TODO: docs
pub enum DeletionOutcome {
    /// TODO: docs
    DeletedAcrossRuns {
        split_start: Option<(ReplicaId, Length, LeafIdx<EditRun>)>,
        split_end: Option<(ReplicaId, Length, LeafIdx<EditRun>)>,
    },

    /// TODO: docs
    DeletedInMiddleOfSingleRun {
        replica_id: ReplicaId,
        range: Range<Length>,
        idx_of_deleted: LeafIdx<EditRun>,
        idx_of_split: LeafIdx<EditRun>,
    },

    /// TODO: docs
    DeletionSplitSingleRun {
        replica_id: ReplicaId,
        offset: Length,
        idx: LeafIdx<EditRun>,
    },

    Wip,
}

/// TODO: docs
#[derive(Clone, PartialEq)]
pub struct EditRun {
    /// TODO: docs
    inserted_at: Anchor,

    /// TODO: docs
    inserted_by: ReplicaId,

    /// TODO: docs
    character_range: Range<Length>,

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
            "{:x}.{:?} L({}) |@ {:?}{}",
            self.inserted_by.as_u32(),
            self.character_range,
            self.lamport_ts,
            self.inserted_at,
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
    fn anchor(&self) -> Anchor {
        self.inserted_at.clone()
    }

    #[inline(always)]
    pub fn end(&self) -> Length {
        self.range().end
    }

    #[inline(always)]
    fn end_mut(&mut self) -> &mut Length {
        &mut self.range_mut().end
    }

    #[inline(always)]
    pub fn extend(&mut self, extend_by: u64) {
        self.character_range.end += extend_by;
    }

    #[inline]
    fn delete_from(&mut self, offset: u64) -> Option<Self> {
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
        Range { start, end }: Range<u64>,
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
    fn delete_up_to(&mut self, offset: u64) -> Option<Self> {
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
    pub fn lamport_ts(&self) -> LamportTimestamp {
        self.lamport_ts
    }

    /// TODO: docs
    #[inline]
    pub fn len(&self) -> u64 {
        self.end() - self.start()
    }

    /// TODO: docs
    #[inline]
    pub fn new(
        inserted_at: Anchor,
        inserted_by: ReplicaId,
        character_range: Range<Length>,
        insertion_ts: InsertionTimestamp,
        lamport_ts: LamportTimestamp,
    ) -> Self {
        Self {
            inserted_at,
            inserted_by,
            character_range,
            insertion_ts,
            lamport_ts,
            is_deleted: false,
        }
    }

    #[inline(always)]
    fn range(&self) -> &Range<Length> {
        &self.character_range
    }

    #[inline(always)]
    fn range_mut(&mut self) -> &mut Range<Length> {
        &mut self.character_range
    }

    #[inline(always)]
    pub fn replica_id(&self) -> ReplicaId {
        self.inserted_by
    }

    /// TODO: docs
    #[inline(always)]
    pub fn split(&mut self, at_offset: u64) -> Option<Self> {
        if at_offset == self.len() || at_offset == 0 {
            None
        } else {
            let mut split = self.clone();
            split.character_range.start += at_offset as u64;
            self.character_range.end = split.character_range.start;
            Some(split)
        }
    }

    #[inline(always)]
    pub fn start(&self) -> Length {
        self.range().start
    }

    #[inline(always)]
    fn start_mut(&mut self) -> &mut Length {
        &mut self.range_mut().start
    }
}

/// TODO: docs
#[derive(Clone, PartialEq)]
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
    #[inline(always)]
    pub fn new(replica_id: ReplicaId, offset: Length) -> Self {
        Self { replica_id, offset }
    }

    #[inline(always)]
    pub fn offset(&self) -> u64 {
        self.offset
    }

    /// A special value used to create an anchor at the start of the document.
    #[inline]
    pub const fn origin() -> Self {
        Self { replica_id: ReplicaId::zero(), offset: 0 }
    }

    #[inline(always)]
    pub fn replica_id(&self) -> ReplicaId {
        self.replica_id
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Diff {
    Add(u64),
    Subtract(u64),
}

impl gtree::Summary for u64 {
    type Diff = Diff;

    #[inline]
    fn empty() -> Self {
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
    fn apply_diff(&mut self, patch: Diff) {
        match patch {
            Diff::Add(add) => *self += add,
            Diff::Subtract(sub) => *self -= sub,
        }
    }
}

impl gtree::Summarize for EditRun {
    type Summary = u64;

    #[inline]
    fn summarize(&self) -> Self::Summary {
        self.len() * (!self.is_deleted as u64)
    }
}

impl gtree::Length<u64> for u64 {
    #[inline]
    fn zero() -> Self {
        0
    }

    #[inline]
    fn len(this: &Self) -> Self {
        *this
    }
}

impl gtree::Join for EditRun {
    #[inline]
    fn append(&mut self, other: Self) -> Option<Self> {
        if self.is_deleted == other.is_deleted
            && self.replica_id() == other.replica_id()
            && self.end() == other.start()
        {
            *self.end_mut() = other.end();
            None
        } else {
            Some(other)
        }
    }

    #[inline]
    fn prepend(&mut self, other: Self) -> Option<Self> {
        if self.is_deleted == other.is_deleted
            && self.replica_id() == other.replica_id()
            && other.end() == self.start()
        {
            *self.start_mut() = other.start();
            None
        } else {
            Some(other)
        }
    }
}

impl gtree::Delete for EditRun {
    fn delete(&mut self) {
        self.is_deleted = true;
    }
}

impl gtree::Leaf for EditRun {
    type Length = u64;
}
