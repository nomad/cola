use alloc::collections::VecDeque;
use core::ops::RangeBounds;
use std::collections::HashMap;

use uuid::Uuid;

use crate::*;

/// TODO: docs
pub type VersionVector = HashMap<ReplicaId, Length>;

/// TODO: docs
pub struct Replica {
    /// TODO: docs
    id: ReplicaId,

    /// TODO: docs
    run_tree: RunTree,

    /// TODO: docs
    run_indices: RunIndices,

    /// TODO: docs
    character_clock: Length,

    /// TODO: docs
    insertion_clock: InsertionClock,

    /// TODO: docs
    lamport_clock: LamportClock,

    /// TODO: docs
    pending: VecDeque<CrdtEdit>,

    /// TODO: docs
    version_vector: VersionVector,
}

impl Replica {
    #[doc(hidden)]
    pub fn assert_invariants(&self) {
        self.run_tree.assert_invariants();
        self.run_indices.assert_invariants(&self.run_tree);
    }

    #[doc(hidden)]
    pub fn average_gtree_inode_occupancy(&self) -> f32 {
        self.run_tree.average_inode_occupancy()
    }

    #[doc(hidden)]
    pub fn debug(&self) -> debug::DebugAsSelf<'_> {
        self.into()
    }

    #[doc(hidden)]
    pub fn debug_as_btree(&self) -> debug::DebugAsBtree<'_> {
        self.into()
    }

    /// TODO: docs
    #[inline]
    pub fn deleted<R>(&mut self, range: R) -> CrdtEdit
    where
        R: RangeBounds<usize>,
    {
        let (start, end) = range_bounds_to_start_end(range, 0, self.len());

        if start == end {
            return CrdtEdit::no_op();
        }

        let deleted_range =
            Range { start: start as Length, end: end as Length };

        let (start, end, outcome) = self.run_tree.delete(deleted_range);

        match outcome {
            DeletionOutcome::DeletedAcrossRuns { split_start, split_end } => {
                if let Some((replica_id, insertion_ts, offset, idx)) =
                    split_start
                {
                    self.run_indices.get_mut(replica_id).split(
                        insertion_ts,
                        offset,
                        idx,
                    );
                }
                if let Some((replica_id, insertion_ts, offset, idx)) =
                    split_end
                {
                    self.run_indices.get_mut(replica_id).split(
                        insertion_ts,
                        offset,
                        idx,
                    );
                }
            },

            DeletionOutcome::DeletedInMiddleOfSingleRun {
                replica_id,
                insertion_ts,
                range,
                idx_of_deleted,
                idx_of_split,
            } => {
                let indices = self.run_indices.get_mut(replica_id);
                indices.split(insertion_ts, range.start, idx_of_deleted);
                indices.split(insertion_ts, range.end, idx_of_split);
            },

            DeletionOutcome::DeletionSplitSingleRun {
                replica_id,
                insertion_ts,
                offset,
                idx,
            } => self.run_indices.get_mut(replica_id).split(
                insertion_ts,
                offset,
                idx,
            ),

            DeletionOutcome::DeletionMergedInPreviousRun {
                replica_id,
                insertion_ts,
                offset,
                deleted,
            } => {
                self.run_indices.get_mut(replica_id).move_len_to_prev_split(
                    insertion_ts,
                    offset,
                    deleted,
                );
            },

            DeletionOutcome::DeletionMergedInNextRun {
                replica_id,
                insertion_ts,
                offset,
                deleted,
            } => {
                self.run_indices.get_mut(replica_id).move_len_to_next_split(
                    insertion_ts,
                    offset,
                    deleted,
                );
            },

            DeletionOutcome::DeletedWholeRun => {},
        }

        CrdtEdit::deletion(
            start,
            end,
            self.id,
            self.character_clock,
            self.version_vector.clone(),
        )
    }

    #[doc(hidden)]
    pub fn empty_leaves(&self) -> (usize, usize) {
        self.run_tree.count_empty_leaves()
    }

    /// TODO: docs
    #[inline]
    pub fn new(len: Length) -> Self {
        let replica_id = ReplicaId::new();

        let mut insertion_clock = InsertionClock::new();

        let mut lamport_clock = LamportClock::new();

        let origin_run = EditRun::new(
            Anchor::origin(),
            replica_id,
            (0..len).into(),
            insertion_clock.next(),
            lamport_clock.next(),
        );

        let (run_tree, origin_idx) = RunTree::new(replica_id, origin_run);

        let run_indices = RunIndices::new(replica_id, origin_idx, len);

        Self {
            id: replica_id,
            run_tree,
            run_indices,
            character_clock: len,
            insertion_clock,
            lamport_clock,
            pending: VecDeque::new(),
            version_vector: VersionVector::new(),
        }
    }

    /// TODO: docs
    #[inline]
    pub fn inserted(&mut self, offset: usize, len: usize) -> CrdtEdit {
        if len == 0 {
            return CrdtEdit::no_op();
        }

        let run_len = len as Length;

        let (anchor, outcome) = self.run_tree.insert(
            offset as Length,
            run_len,
            self.character_clock,
            &mut self.insertion_clock,
            &mut self.lamport_clock,
        );

        self.character_clock += run_len;

        match outcome {
            InsertionOutcome::ExtendedLastRun => {
                self.run_indices.get_mut(self.id).extend_last(run_len)
            },

            InsertionOutcome::SplitRun {
                split_id,
                split_insertion,
                split_at_offset,
                split_idx,
                inserted_idx,
            } => {
                self.run_indices
                    .get_mut(self.id)
                    .append(run_len, inserted_idx);

                self.run_indices.get_mut(split_id).split(
                    split_insertion,
                    split_at_offset,
                    split_idx,
                );
            },

            InsertionOutcome::InsertedRun { inserted_idx } => {
                self.run_indices.get_mut(self.id).append(run_len, inserted_idx)
            },
        };

        CrdtEdit::insertion(
            anchor,
            self.id,
            run_len,
            self.lamport_clock.last(),
        )
    }

    /// TODO: docs
    #[allow(clippy::len_without_is_empty)]
    #[doc(hidden)]
    pub fn len(&self) -> usize {
        self.run_tree.len() as _
    }

    /// TODO: docs
    #[inline]
    pub fn merge(&mut self, crdt_edit: CrdtEdit) -> Option<TextEdit> {
        //match crdt_edit {
        //    Cow::Owned(CrdtEdit {
        //        kind:
        //            CrdtEditKind::Insertion {
        //                content,
        //                id,
        //                anchor,
        //                lamport_ts,
        //                len,
        //            },
        //    }) => self.merge_insertion(
        //        Cow::Owned(content),
        //        id,
        //        anchor,
        //        lamport_ts,
        //    ),

        //    Cow::Borrowed(CrdtEdit {
        //        kind:
        //            CrdtEditKind::Insertion {
        //                content,
        //                id,
        //                anchor,
        //                lamport_ts,
        //                len,
        //            },
        //    }) => self.merge_insertion(
        //        Cow::Borrowed(content.as_str()),
        //        id.clone(),
        //        anchor.clone(),
        //        *lamport_ts,
        //    ),

        //    _ => None,
        //}

        todo!()
    }

    fn merge_insertion(
        &mut self,
        anchor: Anchor,
        lamport_ts: LamportTimestamp,
    ) -> Option<TextEdit> {
        todo!();

        //let Some(run_id) = self.run_indexes.get_run_id(&anchor) else {
        //    let crdt_edit = CrdtEdit::insertion(
        //        content.into_owned(),
        //        id,
        //        anchor,
        //        lamport_ts,
        //    );
        //    self.pending.push_back(crdt_edit);
        //    return None;
        //};

        //let len = M::len(&content);

        //let lamport_ts = self.lamport_clock.update(lamport_ts);

        //let offset = downstream::insert(
        //    &mut self.insertion_runs,
        //    &mut self.run_indexes,
        //    id,
        //    lamport_ts,
        //    anchor,
        //    run_id,
        //    len,
        //);

        //Some(TextEdit::new(content, offset..offset))
    }

    /// TODO: docs
    #[inline]
    pub fn replaced<R, T>(&mut self, _byte_range: R, _text: T) -> CrdtEdit
    where
        R: RangeBounds<usize>,
        T: Into<String>,
    {
        todo!();
    }

    /// TODO: docs
    #[inline]
    pub fn undo(&self, _crdt_edit: &CrdtEdit) -> CrdtEdit {
        todo!();
    }
}

impl core::fmt::Debug for Replica {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        // In the public Debug we just print the ReplicaId to avoid leaking
        // our internals.
        //
        // During development the `Replica::debug()` method (which is public
        // but hidden from the API) can be used to obtain a more useful
        // representation.
        f.debug_tuple("Replica").field(&self.id.0).finish()
    }
}

impl Default for Replica {
    #[inline]
    fn default() -> Self {
        Self::new(0)
    }
}

impl Clone for Replica {
    #[inline(always)]
    fn clone(&self) -> Self {
        let mut lamport_clock = self.lamport_clock;

        lamport_clock.next();

        Self {
            id: ReplicaId::new(),
            run_tree: self.run_tree.clone(),
            character_clock: 0,
            run_indices: self.run_indices.clone(),
            insertion_clock: InsertionClock::new(),
            lamport_clock,
            pending: self.pending.clone(),
            version_vector: self.version_vector.clone(),
        }
    }
}

/// TODO: docs
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReplicaId(Uuid);

impl core::fmt::Debug for ReplicaId {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "ReplicaId({:x})", self.as_u32())
    }
}

impl ReplicaId {
    /// TODO: docs
    #[inline]
    pub fn as_u32(&self) -> u32 {
        self.0.as_fields().0
    }

    /// Creates a new, randomly generated [`ReplicaId`].
    #[inline]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Returns the "nil" id, i.e. the id whose bytes are all zeros.
    ///
    /// This is used to form the [`EditId`] of the first edit run and should
    /// never be used in any of the following user-generated insertion.
    #[inline]
    pub const fn zero() -> Self {
        Self(Uuid::nil())
    }
}

/// TODO: docs
#[derive(Copy, Clone, Default)]
pub struct LamportClock(u64);

impl core::fmt::Debug for LamportClock {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "LamportClock({})", self.0)
    }
}

impl LamportClock {
    #[inline]
    fn last(&self) -> LamportTimestamp {
        self.0.saturating_sub(1)
    }

    #[inline]
    fn new() -> Self {
        Self::default()
    }

    /// TODO: docs
    #[inline]
    pub fn next(&mut self) -> LamportTimestamp {
        let next = self.0;
        self.0 += 1;
        next
    }

    /// TODO: docs
    #[inline]
    fn update(&mut self, other: LamportTimestamp) -> LamportTimestamp {
        self.0 = self.0.max(other) + 1;
        self.0
    }
}

/// TODO: docs
pub type LamportTimestamp = u64;

/// TODO: docs
#[derive(Copy, Clone, Default)]
pub struct InsertionClock(u64);

impl core::fmt::Debug for InsertionClock {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "InsertionClock({})", self.0)
    }
}

impl InsertionClock {
    #[inline]
    fn new() -> Self {
        Self::default()
    }

    /// TODO: docs
    #[inline]
    pub fn next(&mut self) -> InsertionTimestamp {
        let next = self.0;
        self.0 += 1;
        next
    }
}

/// TODO: docs
pub type InsertionTimestamp = u64;

#[inline(always)]
fn range_bounds_to_start_end<R>(
    range: R,
    lo: usize,
    hi: usize,
) -> (usize, usize)
where
    R: RangeBounds<usize>,
{
    use core::ops::Bound;

    let start = match range.start_bound() {
        Bound::Included(&n) => n,
        Bound::Excluded(&n) => n + 1,
        Bound::Unbounded => lo,
    };

    let end = match range.end_bound() {
        Bound::Included(&n) => n + 1,
        Bound::Excluded(&n) => n,
        Bound::Unbounded => hi,
    };

    (start, end)
}

mod debug {
    use core::fmt::Debug;

    use super::*;

    pub struct DebugAsSelf<'a>(BaseDebug<'a, run_tree::DebugAsSelf<'a>>);

    impl<'a> From<&'a Replica> for DebugAsSelf<'a> {
        #[inline]
        fn from(replica: &'a Replica) -> DebugAsSelf<'a> {
            let base = BaseDebug {
                replica,
                debug_run_tree: replica.run_tree.debug_as_self(),
            };

            Self(base)
        }
    }

    impl<'a> core::fmt::Debug for DebugAsSelf<'a> {
        fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
            self.0.fmt(f)
        }
    }

    pub struct DebugAsBtree<'a>(BaseDebug<'a, run_tree::DebugAsBtree<'a>>);

    impl<'a> From<&'a Replica> for DebugAsBtree<'a> {
        #[inline]
        fn from(replica: &'a Replica) -> DebugAsBtree<'a> {
            let base = BaseDebug {
                replica,
                debug_run_tree: replica.run_tree.debug_as_btree(),
            };

            Self(base)
        }
    }

    impl<'a> core::fmt::Debug for DebugAsBtree<'a> {
        fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
            self.0.fmt(f)
        }
    }

    struct BaseDebug<'a, T: Debug> {
        replica: &'a Replica,
        debug_run_tree: T,
    }

    impl<'a, T: Debug> Debug for BaseDebug<'a, T> {
        fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
            let replica = &self.replica;

            f.debug_struct("Replica")
                .field("id", &replica.id)
                .field("run_tree", &self.debug_run_tree)
                .field("run_indices", &replica.run_indices)
                .field("character_clock", &replica.character_clock)
                .field("lamport_clock", &replica.lamport_clock)
                .field("pending", &replica.pending)
                .finish()
        }
    }
}
