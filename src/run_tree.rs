use core::cmp::Ordering;
use core::ops;

use crate::anchor::{Anchor as BiasedAnchor, InnerAnchor as Anchor};
use crate::gtree::LeafIdx;
use crate::*;

const RUN_TREE_ARITY: usize = 32;

type Gtree = crate::Gtree<RUN_TREE_ARITY, EditRun>;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RunTree {
    /// The tree of runs.
    gtree: Gtree,

    /// A secondary data structure that allows to quickly find the
    /// [`LeafIdx`](crate::LeafIdx) of the run that contains a given
    /// [`Anchor`].
    run_indices: RunIndices,
}

impl RunTree {
    #[inline]
    fn append_run_to_another(
        &mut self,
        run: EditRun,
        append_to: LeafIdx<EditRun>,
    ) -> Length {
        let appending_to = self.gtree.leaf(append_to);

        debug_assert!(appending_to.can_append(&run));

        let replica_id = appending_to.replica_id();

        let leaf_len = appending_to.len();

        let run_len = run.len();

        let offset =
            self.gtree.append_leaf_to_another(append_to, run) + leaf_len;

        self.run_indices.get_mut(replica_id).extend_last(run_len);

        offset
    }

    #[inline]
    pub fn assert_invariants(&self) {
        self.gtree.assert_invariants();
        self.run_indices.assert_invariants(self);
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
    pub fn create_anchor(
        &self,
        at_offset: Length,
        with_bias: AnchorBias,
    ) -> BiasedAnchor {
        if at_offset == self.len() && with_bias == AnchorBias::Right {
            return BiasedAnchor::end_of_document();
        }

        if at_offset == 0 {
            if with_bias == AnchorBias::Left {
                return BiasedAnchor::start_of_document();
            }

            let first_run = self
                .gtree
                .leaves_from_first()
                .find_map(|(_, run)| (!run.is_deleted).then_some(run))
                .expect("there's at least one EditRun that's not deleted");

            let anchor = Anchor::new(
                first_run.replica_id(),
                first_run.start(),
                first_run.run_ts(),
            );

            return BiasedAnchor::new(anchor, AnchorBias::Right);
        }

        let (leaf_idx, mut leaf_offset) = self.gtree.leaf_at_offset(at_offset);

        let mut edit_run = self.gtree.leaf(leaf_idx);

        // If the offset is at the end of a run and we're biased to the right
        // then we need to anchor to the start of the next visible run.
        if with_bias == AnchorBias::Right
            && leaf_offset + edit_run.len() == at_offset
        {
            leaf_offset += edit_run.len();

            let next_run = self
                .gtree
                .leaves::<false>(leaf_idx)
                .find_map(|(_, run)| (!run.is_deleted).then_some(run))
                .expect(
                    "we've already handled the case where the offset is at \
                     the end of the document and we're biased to the right",
                );

            edit_run = next_run;
        }

        let inserted_by = edit_run.replica_id();

        let offset = edit_run.start() + at_offset - leaf_offset;

        let contained_in = edit_run.run_ts();

        let anchor = Anchor::new(inserted_by, offset, contained_in);

        BiasedAnchor::new(anchor, with_bias)
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
    pub fn delete(&mut self, range: Range<Length>) -> (Anchor, Anchor) {
        let mut id_start = 0;
        let mut run_ts_start = 0;
        let mut offset_start = 0;

        let mut id_end = 0;
        let mut run_ts_end = 0;
        let mut offset_end = 0;

        let mut split_across_runs = false;

        let delete_from = |run: &mut EditRun, offset: Length| {
            split_across_runs = true;
            id_start = run.replica_id();
            run_ts_start = run.run_ts();
            offset_start = run.start() + offset;
            run.delete_from(offset)
        };

        let delete_up_to = |run: &mut EditRun, offset: Length| {
            id_end = run.replica_id();
            run_ts_end = run.run_ts();
            offset_end = run.start() + offset;
            run.delete_up_to(offset)
        };

        let mut id_range = 0;
        let mut run_ts_range = 0;
        let mut deleted_range_offset = 0;
        let mut deleted_range_run_len = 0;
        let mut deleted_range = Range { start: 0, end: 0 };

        let delete_range = |run: &mut EditRun, range: Range<Length>| {
            id_range = run.replica_id();
            run_ts_range = run.run_ts();
            deleted_range_offset = run.start();
            deleted_range_run_len = run.len();
            deleted_range = range;
            run.delete_range(range)
        };

        let (first_idx, second_idx) =
            self.gtree.delete(range, delete_range, delete_from, delete_up_to);

        if split_across_runs {
            if let Some(idx) = first_idx {
                self.run_indices.get_mut(id_start).split(
                    run_ts_start,
                    offset_start,
                    idx,
                );
            }

            if let Some(idx) = second_idx {
                self.run_indices
                    .get_mut(id_end)
                    .split(run_ts_end, offset_end, idx);
            }

            return (
                Anchor::new(id_start, offset_start, run_ts_start),
                Anchor::new(id_end, offset_end, run_ts_end),
            );
        }

        match (first_idx, second_idx) {
            (Some(first), Some(second)) => {
                let range = deleted_range + deleted_range_offset;
                let indices = self.run_indices.get_mut(id_range);
                indices.split(run_ts_range, range.start, first);
                indices.split(run_ts_range, range.end, second);
            },

            (Some(first), _) => {
                let offset = deleted_range_offset
                    + if deleted_range.start == 0 {
                        deleted_range.end
                    } else {
                        deleted_range.start
                    };

                self.run_indices.get_mut(id_range).split(
                    run_ts_range,
                    offset,
                    first,
                );
            },

            (None, None) if deleted_range.len() < deleted_range_run_len => {
                if deleted_range.start == 0 {
                    self.run_indices.get_mut(id_range).move_len_to_prev_split(
                        run_ts_range,
                        deleted_range_offset + deleted_range.end,
                        deleted_range.len(),
                    );
                } else if deleted_range.end == deleted_range_run_len {
                    self.run_indices.get_mut(id_range).move_len_to_next_split(
                        run_ts_range,
                        deleted_range_offset + deleted_range.start,
                        deleted_range.len(),
                    );
                } else {
                    unreachable!();
                }
            },

            _ => {},
        };

        let anchor_start = Anchor::new(
            id_range,
            deleted_range_offset + deleted_range.start,
            run_ts_range,
        );

        let anchor_end = Anchor::new(
            id_range,
            deleted_range_offset + deleted_range.end,
            run_ts_range,
        );

        (anchor_start, anchor_end)
    }

    #[inline]
    pub fn run(&self, run_idx: LeafIdx<EditRun>) -> &EditRun {
        self.gtree.leaf(run_idx)
    }

    #[inline]
    pub fn insert(
        &mut self,
        offset: Length,
        text: Text,
        run_clock: &mut RunClock,
        lamport_clock: &mut LamportClock,
    ) -> Anchor {
        debug_assert!(!text.range.is_empty());

        let replica_id = text.inserted_by();

        let text_len = text.len();

        if offset == 0 {
            let run = EditRun::new_visible(
                text,
                run_clock.next(),
                lamport_clock.next(),
            );

            let inserted_idx = self.gtree.prepend(run);

            self.run_indices
                .get_mut(replica_id)
                .append(text_len, inserted_idx);

            return Anchor::zero();
        }

        let mut split_id = 0;

        let mut split_insertion = 0;

        let mut split_at_offset = 0;

        let mut anchor = Anchor::zero();

        let insert_with = |run: &mut EditRun, offset: Length| {
            split_id = run.replica_id();
            split_insertion = run.run_ts();
            split_at_offset = run.start() + offset;
            anchor =
                Anchor::new(run.replica_id(), split_at_offset, run.run_ts());

            if run.len() == offset
                && run.end() == text.start()
                && run.replica_id() == text.inserted_by()
                && run.lamport_ts() == lamport_clock.highest()
            {
                run.extend(text.len());
                return (None, None);
            }

            let split = run.split(offset);

            let new_run = EditRun::new_visible(
                text,
                run_clock.next(),
                lamport_clock.next(),
            );

            (Some(new_run), split)
        };

        let (inserted_idx, split_idx) = self.gtree.insert(offset, insert_with);

        if let Some(inserted_idx) = inserted_idx {
            self.run_indices
                .get_mut(replica_id)
                .append(text_len, inserted_idx);

            if let Some(split_idx) = split_idx {
                self.run_indices.get_mut(split_id).split(
                    split_insertion,
                    split_at_offset,
                    split_idx,
                );
            }
        } else {
            self.run_indices.get_mut(replica_id).extend_last(text_len)
        }

        anchor
    }

    #[inline]
    fn insert_run_after_another(
        &mut self,
        run: EditRun,
        insert_after: LeafIdx<EditRun>,
    ) -> Length {
        let run_ts = run.run_ts();

        let replica_id = run.replica_id();

        let run_len = run.len();

        let (offset, idx) =
            self.gtree.insert_leaf_after_another(run, insert_after);

        let indices = self.run_indices.get_mut(replica_id);

        if run_ts + 1 == indices.len() as RunTs {
            indices.append_to_last(run_len, idx);
        } else {
            indices.append(run_len, idx);
        };

        offset
    }

    #[inline]
    fn insert_run_at_zero(&mut self, run: EditRun) -> Length {
        let replica_id = run.replica_id();

        let mut leaves = self.gtree.leaves_from_first();

        let (mut prev_idx, first_leaf) = leaves.next().unwrap();

        if run < *first_leaf {
            let run_len = run.len();
            let idx = self.gtree.prepend(run);
            self.run_indices.get_mut(replica_id).append(run_len, idx);
            return 0;
        }

        for (idx, leaf) in leaves {
            if run > *leaf {
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

    /// Deletes a range of a run, returning the `LeafIdx` of the `EditRun` that
    /// immediately follows the run containing the deleted range.
    #[inline]
    fn delete_leaf_range(
        &mut self,
        leaf_idx: LeafIdx<EditRun>,
        leaf_offset: Length,
        range: Range<Length>,
    ) -> LeafIdx<EditRun> {
        let run = self.gtree.leaf(leaf_idx);

        debug_assert!(!run.is_deleted);

        let id_range = run.replica_id();
        let run_ts_range = run.run_ts();
        let deleted_range_offset = run.start();
        let deleted_range_run_len = run.len();

        let (first_idx, second_idx) = self.gtree.delete_leaf_range(
            leaf_idx,
            leaf_offset,
            range,
            |run, range| run.delete_range(range),
        );

        match (first_idx, second_idx) {
            (Some(first), Some(second)) => {
                let range = range + deleted_range_offset;
                let indices = self.run_indices.get_mut(id_range);
                indices.split(run_ts_range, range.start, first);
                indices.split(run_ts_range, range.end, second);
                second
            },

            (Some(first), _) => {
                let (next_idx, offset) = if range.start == 0 {
                    (first, range.end)
                } else {
                    (self.gtree.next_leaf(first).unwrap_or(first), range.start)
                };

                self.run_indices.get_mut(id_range).split(
                    run_ts_range,
                    deleted_range_offset + offset,
                    first,
                );

                next_idx
            },

            (None, None) if range.len() < deleted_range_run_len => {
                if range.start == 0 {
                    self.run_indices.get_mut(id_range).move_len_to_prev_split(
                        run_ts_range,
                        deleted_range_offset + range.end,
                        range.len(),
                    );

                    leaf_idx
                } else if range.end == deleted_range_run_len {
                    self.run_indices.get_mut(id_range).move_len_to_next_split(
                        run_ts_range,
                        deleted_range_offset + range.start,
                        range.len(),
                    );

                    self.gtree.next_leaf(leaf_idx).unwrap()
                } else {
                    unreachable!();
                }
            },

            _ => self.gtree.next_leaf(leaf_idx).unwrap_or(leaf_idx),
        }
    }

    #[inline]
    pub fn merge_deletion(
        &mut self,
        deletion: &Deletion,
    ) -> Vec<ops::Range<usize>> {
        let start_idx = if deletion.start().is_zero() {
            // If the deletion starts at the beginning of the document we start
            // from the first run that was visible when the deletion was made.
            self.gtree
                .leaves_from_first()
                .find_map(|(run_idx, run)| {
                    (run.start()
                        < deletion.version_map().get(run.replica_id()))
                    .then_some(run_idx)
                })
                .unwrap()
        } else {
            self.run_indices.idx_at_anchor(deletion.start(), AnchorBias::Right)
        };

        let mut leaf_offset = self.gtree.offset_of_leaf(start_idx);

        let start = self.gtree.leaf(start_idx);

        let mut ranges = Vec::new();

        if start.contains_anchor(deletion.end()) {
            let run = start;

            if !run.is_deleted {
                let delete_from = if deletion.start().is_zero() {
                    0
                } else {
                    deletion.start().offset() - run.start()
                };

                let delete_up_to = deletion.end().offset() - run.start();
                let delete_range = (delete_from..delete_up_to).into();
                self.delete_leaf_range(start_idx, leaf_offset, delete_range);
                ranges.push((delete_range + leaf_offset).into());
            }

            return ranges;
        }

        let end_idx =
            self.run_indices.idx_at_anchor(deletion.end(), AnchorBias::Left);

        /// TODO: docs
        enum DeletionState {
            Deleting(Length),
            Skipping,
            Starting,
        }

        let mut visible_offset = leaf_offset;

        let (start_idx, mut state) = if start.is_deleted {
            let next_idx = self.gtree.next_leaf(start_idx).unwrap();
            (next_idx, DeletionState::Starting)
        } else {
            let delete_from = if deletion.start().is_zero() {
                0
            } else {
                deletion.start().offset() - start.start()
            };

            let deleted_up_to = deletion.version_map().get(start.replica_id());

            if start.end() > deleted_up_to {
                let delete_up_to = deleted_up_to - start.start();

                let next_idx = self.delete_leaf_range(
                    start_idx,
                    leaf_offset,
                    (delete_from..delete_up_to).into(),
                );

                let deletion_start = leaf_offset + delete_from;
                let deletion_end = leaf_offset + delete_up_to;
                ranges.push(deletion_start..deletion_end);

                leaf_offset += delete_from;

                visible_offset += delete_up_to;

                (next_idx, DeletionState::Skipping)
            } else {
                let len = start.len();

                let next_idx = self.delete_leaf_range(
                    start_idx,
                    leaf_offset,
                    (delete_from..len).into(),
                );

                leaf_offset += delete_from;

                visible_offset += len;

                (next_idx, DeletionState::Deleting(leaf_offset))
            }
        };

        let mut runs = self.gtree.leaves::<true>(start_idx);

        loop {
            let (run_idx, run) = runs.next().unwrap();

            if run_idx == end_idx {
                if run.is_deleted {
                    self.gtree.remove_cursor();

                    if let DeletionState::Deleting(start_offset) = state {
                        ranges.push(start_offset..visible_offset);
                    }
                } else {
                    let delete_up_to = deletion.end().offset() - run.start();

                    self.delete_leaf_range(
                        end_idx,
                        leaf_offset,
                        (0..delete_up_to).into(),
                    );

                    let deletion_start =
                        if let DeletionState::Deleting(start) = state {
                            start
                        } else {
                            visible_offset
                        };

                    let deletion_end = visible_offset + delete_up_to;

                    ranges.push(deletion_start..deletion_end);
                }
                break;
            }

            if run.is_deleted {
                continue;
            }

            let run_len = run.len();

            let deleted_up_to = deletion.version_map().get(run.replica_id());

            if run.end() > deleted_up_to {
                if run.start() >= deleted_up_to {
                    if let DeletionState::Deleting(start_offset) = state {
                        ranges.push(start_offset..visible_offset);
                    }

                    state = DeletionState::Skipping;

                    leaf_offset += run_len;

                    visible_offset += run_len;
                } else {
                    let delete_up_to = deleted_up_to - run.start();

                    let next_idx = self.delete_leaf_range(
                        run_idx,
                        leaf_offset,
                        (0..delete_up_to).into(),
                    );

                    let deletion_start =
                        if let DeletionState::Deleting(start) = state {
                            start
                        } else {
                            visible_offset
                        };

                    let deletion_end = visible_offset + delete_up_to;

                    ranges.push(deletion_start..deletion_end);

                    state = DeletionState::Skipping;

                    visible_offset += delete_up_to;

                    runs = self.gtree.leaves::<true>(next_idx);
                }

                continue;
            }

            // This is awful but I'm not sure it can be avoided. We
            // transmute a shared reference to the Gtree to an exclusive
            // one to modify it while we're iterating over it.
            //
            // SAFETY: this is safe because deleting a whole run doesn't
            // modify the Gtree's structure at all (no heap allocations,
            // etc), only the lengths of all the nodes from the run we're
            // deleting up to the root.
            let gtree = unsafe {
                #[allow(mutable_transmutes)]
                core::mem::transmute::<_, &mut Gtree>(&self.gtree)
            };

            gtree.with_leaf_mut(run_idx, EditRun::delete);

            gtree.remove_cursor();

            if !matches!(state, DeletionState::Deleting(_)) {
                state = DeletionState::Deleting(visible_offset);
            }

            visible_offset += run_len;
        }

        ranges
    }

    #[inline]
    pub fn merge_insertion(&mut self, insertion: &Insertion) -> Length {
        let run = EditRun::from_insertion(insertion);

        if insertion.anchor().is_zero() {
            return self.insert_run_at_zero(run);
        }

        let anchor_idx = self
            .run_indices
            .idx_at_anchor(insertion.anchor(), AnchorBias::Left);

        let anchor = self.gtree.leaf(anchor_idx);

        // If the insertion is anchored in the middle of the anchor run then
        // there can't be any other runs that are tied with it. In this case we
        // can just split the anchor run and insert the new run after it.
        if insertion.anchor().offset() < anchor.end() {
            let insert_at = insertion.anchor().offset() - anchor.start();
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
            if run > *next_sibling {
                prev_idx = idx;

                for (idx, sibling) in siblings {
                    if run > *sibling {
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
            if run > *leaf {
                prev_idx = idx;
            } else {
                return self.insert_run_after_another(run, prev_idx);
            }
        }

        // If we get here we're inserting after the last run in the Gtree.
        self.insert_run_after_another(run, prev_idx)
    }

    #[inline]
    pub fn new(first_run: EditRun) -> Self {
        let id = first_run.replica_id();
        let len = first_run.len();
        let (gtree, idx) = Gtree::from_first_leaf(first_run);
        let mut run_indices = RunIndices::new();
        run_indices.get_mut(id).append(len, idx);
        Self { gtree, run_indices }
    }

    #[inline]
    pub fn resolve_anchor(&self, anchor: BiasedAnchor) -> Length {
        if anchor.is_start_of_document() {
            return 0;
        } else if anchor.is_end_of_document() {
            return self.len();
        }

        let anchor_idx =
            self.run_indices.idx_at_anchor(anchor.inner(), anchor.bias());

        let anchor_offset = self.gtree.offset_of_leaf(anchor_idx);

        let run_containing_anchor = self.gtree.leaf(anchor_idx);

        let offset_in_anchor = if run_containing_anchor.is_deleted {
            0
        } else {
            anchor.inner().offset() - run_containing_anchor.start()
        };

        anchor_offset + offset_in_anchor
    }

    #[inline]
    pub fn run_indices(&self) -> &RunIndices {
        &self.run_indices
    }

    #[inline]
    fn split_run_with_another(
        &mut self,
        run: EditRun,
        insert_into: LeafIdx<EditRun>,
        at_offset: Length,
    ) -> Length {
        debug_assert!(at_offset > 0);
        let splitting = self.gtree.leaf(insert_into);

        debug_assert!(at_offset < splitting.len());

        let split_id = splitting.replica_id();
        let split_insertion = splitting.run_ts();
        let split_at_offset = splitting.start() + at_offset;

        let run_len = run.len();

        let inserted_id = run.replica_id();

        let (offset, inserted_idx, split_idx) =
            self.gtree.split_leaf_with_another(insert_into, |splitting| {
                let split = splitting.split(at_offset).unwrap();
                (run, split)
            });

        self.run_indices.get_mut(inserted_id).append(run_len, inserted_idx);

        self.run_indices.get_mut(split_id).split(
            split_insertion,
            split_at_offset,
            split_idx,
        );

        offset
    }
}

/// TODO: docs
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct EditRun {
    /// TODO: docs
    text: Text,

    /// TODO: docs
    run_ts: RunTs,

    /// TODO: docs
    lamport_ts: LamportTs,

    /// TODO: docs
    is_deleted: bool,
}

impl core::fmt::Debug for EditRun {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(
            f,
            "{:?} L({}) I({}){}",
            self.text,
            self.lamport_ts,
            self.run_ts,
            if self.is_deleted { " 🪦" } else { "" },
        )
    }
}

impl Ord for EditRun {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        // We first sort descending on Lamport timestamps, using the replica id
        // as a tie breaker (the ids are sorted in ascending order but that's
        // totally arbitrary).

        self.lamport_ts
            .cmp(&other.lamport_ts)
            .reverse()
            .then(self.replica_id().cmp(&other.replica_id()))
    }
}

impl PartialOrd for EditRun {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl EditRun {
    #[inline]
    pub fn can_append(&self, other: &Self) -> bool {
        self.end() == other.start()
            && self.replica_id() == other.replica_id()
            && self.lamport_ts() == other.lamport_ts()
            && self.run_ts() == other.run_ts()
            && self.is_deleted == other.is_deleted
    }

    #[inline]
    pub fn can_prepend(&self, other: &Self) -> bool {
        self.start() == other.end()
            && self.replica_id() == other.replica_id()
            && self.lamport_ts() == other.lamport_ts()
            && self.run_ts() == other.run_ts()
            && self.is_deleted == other.is_deleted
    }

    #[inline]
    fn contains_anchor(&self, anchor: Anchor) -> bool {
        debug_assert!(!anchor.is_zero());

        self.replica_id() == anchor.replica_id()
            && self.text.start() < anchor.offset()
            && self.text.end() >= anchor.offset()
    }

    #[inline(always)]
    pub fn end(&self) -> Length {
        self.text.end()
    }

    #[inline(always)]
    fn end_mut(&mut self) -> &mut Length {
        &mut self.text.range.end
    }

    #[inline(always)]
    pub fn extend(&mut self, extend_by: Length) {
        self.text.range.end += extend_by;
    }

    #[inline(always)]
    fn delete(&mut self) {
        self.is_deleted = true;
    }

    #[inline]
    fn delete_from(&mut self, offset: Length) -> Option<Self> {
        if offset == 0 {
            self.delete();
            None
        } else if offset < self.len() {
            let mut del = self.split(offset)?;
            del.delete();
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
                d.delete();
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
            self.delete();
            rest
        } else {
            self.delete();
            None
        }
    }

    #[inline(always)]
    pub fn from_insertion(insertion: &Insertion) -> Self {
        Self {
            text: insertion.text().clone(),
            run_ts: insertion.run_ts(),
            lamport_ts: insertion.lamport_ts(),
            is_deleted: false,
        }
    }

    #[inline(always)]
    pub fn lamport_ts(&self) -> LamportTs {
        self.lamport_ts
    }

    /// TODO: docs
    #[inline]
    pub fn len(&self) -> Length {
        self.text.len()
    }

    /// TODO: docs
    #[inline(always)]
    pub fn new_visible(
        text: Text,
        run_ts: RunTs,
        lamport_ts: LamportTs,
    ) -> Self {
        Self::new(text, run_ts, lamport_ts, false)
    }

    /// TODO: docs
    #[inline(always)]
    pub fn new(
        text: Text,
        run_ts: RunTs,
        lamport_ts: LamportTs,
        is_deleted: bool,
    ) -> Self {
        Self { text, run_ts, lamport_ts, is_deleted }
    }

    /// Returns the [`ReplicaId`] of the replica that inserted this run.
    #[inline(always)]
    pub fn replica_id(&self) -> ReplicaId {
        self.text.inserted_by()
    }

    #[inline(always)]
    pub fn run_ts(&self) -> RunTs {
        self.run_ts
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
        self.text.start()
    }

    #[inline(always)]
    fn start_mut(&mut self) -> &mut Length {
        &mut self.text.range.start
    }

    #[inline]
    fn visible_len(&self) -> Length {
        self.len() * (!self.is_deleted as Length)
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
            debug_assert_eq!(self.run_ts, other.run_ts);
            *self.start_mut() = other.start();
            Ok(())
        } else {
            Err(other)
        }
    }
}

impl gtree::Delete for EditRun {
    fn delete(&mut self) {
        self.delete();
    }
}

impl gtree::Leaf for EditRun {
    #[inline]
    fn len(&self) -> Length {
        self.visible_len()
    }
}

pub(crate) type DebugAsBtree<'a> =
    gtree::DebugAsBtree<'a, RUN_TREE_ARITY, EditRun>;

pub(crate) type DebugAsSelf<'a> =
    gtree::DebugAsSelf<'a, RUN_TREE_ARITY, EditRun>;

#[cfg(feature = "encode")]
mod encode {
    use core::mem;

    use super::*;
    use crate::encode::{
        BoolDecodeError,
        Decode,
        DecodeWithCtx,
        Encode,
        IntDecodeError,
    };
    use crate::gtree::{encode::InodeDecodeError, Inode, InodeIdx, Lnode};
    use crate::run_indices::{Fragment, Fragments, ReplicaIndices};

    impl Encode for RunTree {
        #[inline]
        fn encode(&self, buf: &mut Vec<u8>) {
            let indices = self.run_indices.iter();

            (indices.len() as u64).encode(buf);

            for (&replica_id, indices) in indices {
                ReplicaRuns::new(replica_id, indices, &self.gtree).encode(buf);
            }

            let inodes = self.gtree.inodes();

            (inodes.len() as u64).encode(buf);

            for inode in inodes {
                inode.encode(buf);
            }

            self.gtree.root_idx().encode(buf);
        }
    }

    pub(crate) enum RunTreeDecodeError {
        Bool(BoolDecodeError),
        Inode(InodeDecodeError),
        Int(IntDecodeError),
    }

    impl From<BoolDecodeError> for RunTreeDecodeError {
        #[inline(always)]
        fn from(err: BoolDecodeError) -> Self {
            Self::Bool(err)
        }
    }

    impl From<InodeDecodeError> for RunTreeDecodeError {
        #[inline(always)]
        fn from(err: InodeDecodeError) -> Self {
            Self::Inode(err)
        }
    }

    impl From<IntDecodeError> for RunTreeDecodeError {
        #[inline(always)]
        fn from(err: IntDecodeError) -> Self {
            Self::Int(err)
        }
    }

    impl core::fmt::Display for RunTreeDecodeError {
        #[inline]
        fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
            let err: &dyn core::fmt::Display = match self {
                Self::Bool(err) => err,
                Self::Inode(err) => err,
                Self::Int(err) => err,
            };

            write!(f, "RunTree: couldn't be decoded: {err}")
        }
    }

    #[derive(Default)]
    struct RunTreeDecodeCtx {
        lnodes: Vec<Lnode<EditRun>>,
        run_indices: RunIndices,
        replica_indices: Vec<(Fragments, Length)>,
        fragments: Fragments,
        replica_id: ReplicaId,
        run_ts: RunTs,
        temporal_offset: Length,
    }

    impl Decode for RunTree {
        type Value = Self;

        type Error = RunTreeDecodeError;

        #[inline]
        fn decode(buf: &[u8]) -> Result<(Self, &[u8]), Self::Error> {
            let (num_replicas, mut buf) = u64::decode(buf)?;

            let mut ctx = RunTreeDecodeCtx::default();

            for _ in 0..num_replicas {
                ((), buf) = ReplicaRuns::decode(buf, &mut ctx)?;
                ctx.replica_indices.clear();
            }

            let RunTreeDecodeCtx { lnodes, run_indices, .. } = ctx;

            let (num_inodes, mut buf) = u64::decode(buf)?;

            let mut inodes = Vec::new();

            for _ in 0..num_inodes {
                let (inode, new_buf) = Inode::decode(buf)?;
                inodes.push(inode);
                buf = new_buf;
            }

            let (root_idx, buf) = InodeIdx::decode(buf)?;

            let gtree = Gtree::new(inodes, lnodes, root_idx);

            let this = Self { gtree, run_indices };

            Ok((this, buf))
        }
    }

    struct ReplicaRuns<'a> {
        replica_id: ReplicaId,
        runs: &'a ReplicaIndices,
        gtree: &'a Gtree,
    }

    impl<'a> ReplicaRuns<'a> {
        #[inline(always)]
        fn new(
            replica_id: ReplicaId,
            runs: &'a ReplicaIndices,
            gtree: &'a Gtree,
        ) -> Self {
            Self { replica_id, runs, gtree }
        }
    }

    impl Encode for ReplicaRuns<'_> {
        #[inline(always)]
        fn encode(&self, buf: &mut Vec<u8>) {
            self.replica_id.encode(buf);
            (self.runs.len() as RunTs).encode(buf);

            for (fragments, _) in self.runs.iter() {
                RunFragments::new(fragments, self.gtree).encode(buf);
            }
        }
    }

    impl DecodeWithCtx for ReplicaRuns<'_> {
        type Value = ();

        type Error = RunTreeDecodeError;

        type Ctx = RunTreeDecodeCtx;

        #[inline]
        fn decode<'buf>(
            buf: &'buf [u8],
            ctx: &mut Self::Ctx,
        ) -> Result<(Self::Value, &'buf [u8]), Self::Error> {
            let (replica_id, buf) = ReplicaId::decode(buf)?;

            let (num_runs, mut buf) = RunTs::decode(buf)?;

            ctx.replica_id = replica_id;

            ctx.temporal_offset = 0;

            for run_ts in 0..num_runs {
                ctx.run_ts = run_ts;
                ((), buf) = RunFragments::decode(buf, ctx)?;
            }

            let indices = mem::take(&mut ctx.replica_indices);

            *ctx.run_indices.get_mut(replica_id) =
                ReplicaIndices::new(indices);

            Ok(((), buf))
        }
    }

    struct RunFragments<'a> {
        fragments: &'a Fragments,
        gtree: &'a Gtree,
    }

    impl<'a> RunFragments<'a> {
        #[inline(always)]
        fn new(fragments: &'a Fragments, gtree: &'a Gtree) -> Self {
            Self { fragments, gtree }
        }
    }

    impl Encode for RunFragments<'_> {
        #[inline(always)]
        fn encode(&self, buf: &mut Vec<u8>) {
            (self.fragments.num_fragments() as u64).encode(buf);

            for fragment in self.fragments.iter() {
                RunFragment::new(fragment.leaf_idx(), self.gtree).encode(buf);
            }
        }
    }

    impl DecodeWithCtx for RunFragments<'_> {
        type Value = ();

        type Error = RunTreeDecodeError;

        type Ctx = RunTreeDecodeCtx;

        #[inline]
        fn decode<'buf>(
            buf: &'buf [u8],
            ctx: &mut Self::Ctx,
        ) -> Result<(Self::Value, &'buf [u8]), Self::Error> {
            let (num_fragments, mut buf) = u64::decode(buf)?;

            let initial_temporal_offset = ctx.temporal_offset;

            for _ in 0..num_fragments {
                ((), buf) = RunFragment::decode(buf, ctx)?;
            }

            let fragments = mem::take(&mut ctx.fragments);

            ctx.replica_indices.push((fragments, initial_temporal_offset));

            Ok(((), buf))
        }
    }

    struct RunFragment<'a> {
        leaf_idx: LeafIdx<EditRun>,
        gtree: &'a Gtree,
    }

    impl<'a> RunFragment<'a> {
        #[inline(always)]
        fn new(leaf_idx: LeafIdx<EditRun>, gtree: &'a Gtree) -> Self {
            Self { leaf_idx, gtree }
        }
    }

    impl Encode for RunFragment<'_> {
        #[inline(always)]
        fn encode(&self, buf: &mut Vec<u8>) {
            let edit_run = self.gtree.leaf(self.leaf_idx);

            edit_run.text.len().encode(buf);
            edit_run.lamport_ts.encode(buf);
            edit_run.is_deleted.encode(buf);
            self.leaf_idx.encode(buf);
            self.gtree.parent(self.leaf_idx).encode(buf);
        }
    }

    impl DecodeWithCtx for RunFragment<'_> {
        type Value = ();

        type Error = RunTreeDecodeError;

        type Ctx = RunTreeDecodeCtx;

        #[inline]
        fn decode<'buf>(
            buf: &'buf [u8],
            ctx: &mut Self::Ctx,
        ) -> Result<(Self::Value, &'buf [u8]), Self::Error> {
            let (len, buf) = Length::decode(buf)?;
            let (lamport_ts, buf) = LamportTs::decode(buf)?;
            let (is_deleted, buf) = bool::decode(buf)?;
            let (leaf_idx, buf) = LeafIdx::<EditRun>::decode(buf)?;
            let (parent_idx, buf) = InodeIdx::decode(buf)?;

            ctx.fragments.append(Fragment::new(len, leaf_idx));

            let temporal_start = ctx.temporal_offset;

            ctx.temporal_offset += len;

            let temporal_end = ctx.temporal_offset;

            let text = Text::new(ctx.replica_id, temporal_start..temporal_end);

            let edit_run =
                EditRun::new(text, ctx.run_ts, lamport_ts, is_deleted);

            ctx.lnodes.push(Lnode::new(edit_run, parent_idx));

            Ok(((), buf))
        }
    }
}

#[cfg(feature = "serde")]
mod serde {
    crate::encode::impl_serialize!(super::RunTree);
    crate::encode::impl_deserialize!(super::RunTree);
}
