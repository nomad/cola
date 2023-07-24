use core::cmp::Ordering;
use core::ops;

use crate::gtree::LeafIdx;
use crate::*;

const RUN_TREE_ARITY: usize = 32;

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "encode", derive(serde::Serialize, serde::Deserialize))]
pub(crate) struct RunTree {
    /// The tree of runs.
    gtree: Gtree<RUN_TREE_ARITY, EditRun>,

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
    ) -> (Anchor, RunTs, Anchor, RunTs) {
        let mut id_start = ReplicaId::zero();
        let mut run_ts_start = 0;
        let mut offset_start = 0;

        let mut id_end = ReplicaId::zero();
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

        let mut id_range = ReplicaId::zero();
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
                Anchor::new(id_start, offset_start),
                run_ts_start,
                Anchor::new(id_end, offset_end),
                run_ts_end,
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

        let anchor_start =
            Anchor::new(id_range, deleted_range_offset + deleted_range.start);

        let anchor_end =
            Anchor::new(id_range, deleted_range_offset + deleted_range.end);

        (anchor_start, run_ts_range, anchor_end, run_ts_range)
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
    ) -> (Anchor, RunTs) {
        debug_assert!(!text.range.is_empty());

        let replica_id = text.inserted_by();

        let mut anchor_ts = 0;

        let text_len = text.len();

        if offset == 0 {
            let run = EditRun::new(
                Anchor::origin(),
                text,
                run_clock.next(),
                lamport_clock.next(),
            );

            let inserted_idx = self.gtree.prepend(run);

            self.run_indices
                .get_mut(replica_id)
                .append(text_len, inserted_idx);

            return (Anchor::origin(), anchor_ts);
        }

        let mut split_id = ReplicaId::zero();

        let mut split_insertion = 0;

        let mut split_at_offset = 0;

        let mut anchor = Anchor::origin();

        let insert_with = |run: &mut EditRun, offset: Length| {
            split_id = run.replica_id();
            split_insertion = run.run_ts();
            split_at_offset = run.start() + offset;
            anchor_ts = run.run_ts();
            anchor = Anchor::new(run.replica_id(), split_at_offset);

            if run.len() == offset
                && run.replica_id() == text.inserted_by()
                && run.end() == text.range.start
            {
                run.extend(text.len());
                return (None, None);
            }

            let split = run.split(offset);

            let new_run = EditRun::new(
                anchor,
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

        (anchor, anchor_ts)
    }

    #[inline]
    fn insert_run_after_another(
        &mut self,
        run: EditRun,
        insert_after: LeafIdx<EditRun>,
    ) -> Length {
        let append_to_last = !run.anchor().is_zero()
            && run.anchor().replica_id() == run.replica_id()
            && run.anchor().offset == run.start();

        let replica_id = run.replica_id();

        let run_len = run.len();

        let (offset, idx) =
            self.gtree.insert_leaf_after_another(run, insert_after);

        let indices = self.run_indices.get_mut(replica_id);

        if append_to_last {
            indices.append_to_last(run_len, idx);
        } else {
            indices.append(run_len, idx);
        };

        offset
    }

    #[inline]
    fn insert_run_at_origin(&mut self, run: EditRun) -> Length {
        debug_assert!(run.anchor().is_zero());

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

    #[inline]
    fn delete_leaf(&mut self, leaf_idx: LeafIdx<EditRun>) {
        // 1. update cursor

        todo!();
    }

    #[inline]
    fn delete_leaf_range(
        &mut self,
        leaf_idx: LeafIdx<EditRun>,
        range: Range<Length>,
    ) {
        // 1. if range starts at 0 try to merge it to previous run
        // 2. if range ends at len try to merge it to next run
        // 3. update cursor

        todo!();
    }

    #[inline]
    fn stuff<'a, I>(
        &mut self,
        mut runs: I,
        start_idx: LeafIdx<EditRun>,
        end_idx: LeafIdx<EditRun>,
        version_map: &VersionMap,
    ) -> Outcome
    where
        I: Iterator<Item = (LeafIdx<EditRun>, &'a EditRun)>,
    {
        todo!();

        // let mut len_skipped = 0;
        //
        // let mut len_deleted = 0;

        // AAAAAAAAAAAAAAA this shit is fucking impossible to write.
        //
        // end -> return
        //
        // Deleted -> always skip
        //
        // Ends over
        //   -> start is also over
        //      -> is_first_whole || last_was_whole -> add to len_skipped cntn
        //      -> not -> return
        //
        //   -> start is not over -> return
        //
        // Whole run is visible and present at deletion -> delete leaf and add
        // to
        //
        //
        // DOOOOH
        //

        // loop {
        //     let Some((run_idx, run)) = runs.next() else {
        //         return OutCome::IteratorEnded();
        //         // return Stuff::Finish(len_deleted);
        //     };
        //
        //     if run_idx == end_idx {
        //         return Stuff::Done(len_deleted);
        //     }
        //
        //     if run.is_deleted {
        //         continue;
        //     }
        //
        //     let delete_up_to = version_map.get(run.replica_id());
        //
        //     if run.end() > delete_up_to {
        //         if run.start() >= delete_up_to && len_deleted == 0 {
        //             len_skipped += run.len();
        //         } else {
        //             todo!();
        //             // return Stuff::Dont((run_idx, len_deleted, delete_up_to));
        //         }
        //     }
        //
        //     len_deleted += run.len();
        //
        //     self.delete_leaf(run_idx);
        // }
    }

    #[inline]
    pub fn merge_deletion(
        &mut self,
        deletion: &Deletion,
    ) -> Option<MergedDeletion> {
        let start_idx = if deletion.start().is_zero() {
            todo!();
        } else {
            self.run_indices
                .idx_at_anchor(deletion.start(), deletion.start_ts())
        };

        let mut visible_offset = self.gtree.offset_of_leaf(start_idx);

        let start = self.gtree.leaf(start_idx);

        if start.contains(deletion.end()) {
            let run = start;

            if run.is_deleted {
                return None;
            } else {
                let start = deletion.start().offset - run.start();
                let end = deletion.end().offset - run.start();
                let range = (start..end).into();
                self.delete_leaf_range(start_idx, range);
                return Some(MergedDeletion::Contiguous(
                    (range + visible_offset).into(),
                ));
            }
        }

        let end_idx =
            self.run_indices.idx_at_anchor(deletion.end(), deletion.end_ts());

        let mut deletion_run =
            if start.is_deleted || start.end() == deletion.start().offset {
                DeletionRun::Starting
            } else {
                let delete_from = deletion.start().offset - start.start();
                visible_offset += delete_from;
                let len = start.len();
                self.delete_leaf_range(start_idx, (delete_from..len).into());
                DeletionRun::Deleting(visible_offset + delete_from)
            };

        let mut runs = self.gtree.leaves::<false>(start_idx);

        let mut ranges = Vec::new();

        loop {
            let (run_idx, run) = runs.next().unwrap();

            if run_idx == end_idx {
                if run.is_deleted {
                    todo!();
                    break;
                } else {
                    todo!();
                    break;
                }
            }

            if run.is_deleted {
                continue;
            }

            let run_len = run.len();

            let deleted_up_to = deletion.version_map().get(run.replica_id());

            if run.end() > deleted_up_to {
                if run.start() >= deleted_up_to {
                    match deletion_run {
                        DeletionRun::Deleting(start_offset) => {
                            ranges.push(start_offset..visible_offset);

                            deletion_run =
                                DeletionRun::Skipping(visible_offset);
                        },

                        DeletionRun::Starting => {
                            deletion_run =
                                DeletionRun::Skipping(visible_offset);
                        },

                        DeletionRun::Skipping(_) => {},
                    }
                } else {
                    let delete_up_to = deleted_up_to - run.start();

                    self.delete_leaf_range(run_idx, (0..delete_up_to).into());

                    let deletion_start =
                        if let DeletionRun::Deleting(start) = deletion_run {
                            start
                        } else {
                            visible_offset
                        };

                    let deletion_end = visible_offset + delete_up_to;

                    ranges.push(deletion_start..deletion_end);

                    deletion_run = DeletionRun::Skipping(deletion_end);

                    runs = self.gtree.leaves::<false>(run_idx);
                }
            }

            if !matches!(deletion_run, DeletionRun::Deleting(_)) {
                deletion_run = DeletionRun::Deleting(visible_offset);
            }

            visible_offset += run_len;
        }

        match ranges.len() {
            0 => None,

            1 => Some(MergedDeletion::Contiguous(
                ranges.into_iter().next().unwrap(),
            )),

            _ => Some(MergedDeletion::Split(ranges)),
        }
    }

    #[inline]
    pub fn merge_deletion_2(
        &self,
        deletion: &Deletion,
    ) -> Option<MergedDeletion> {
        todo!();

        // let mut start_idx = if deletion.start().is_zero() {
        //     todo!();
        // } else {
        //     self.run_indices
        //         .idx_at_anchor(deletion.start(), deletion.start_ts())
        // };
        //
        // let start_offset = self.gtree.offset_of_leaf(start_idx);
        //
        // let mut start = self.gtree.leaf(start_idx);
        //
        // let end_idx =
        //     self.run_indices.idx_at_anchor(deletion.end(), deletion.end_ts());
        //
        // let mut state = DeletionState::Began(start_offset);
        //
        // let start_idx = loop {
        //     let siblings = self.gtree.siblings::<true>(start_idx);
        //
        //     let (outcome, last_idx) =
        //         self.stuff(siblings, end_idx, deletion.version_map());
        //
        //     start_idx = self.update_state(&mut state, outcome, last_idx);
        //
        //     match state {
        //         DeletionState::Done(result) => return result,
        //         DeletionState::Next => break start_idx,
        //         _ => continue,
        //     }
        // };
        //
        // loop {
        //     let leaves = self.gtree.leaves::<false>(start_idx);
        //
        //     let (outcome, last_idx) =
        //         self.stuff(leaves, end_idx, deletion.version_map());
        //
        //     start_idx = self.update_state(&mut state, outcome, last_idx);
        //
        //     if let DeletionState::Done(result) = state {
        //         return result;
        //     }
        // }

        // 'outer: loop {
        //     let mut last_idx = start_idx;
        //
        //     for (idx, run) in self.gtree.siblings::<true>(start_idx) {
        //         if !run.is_deleted {
        //             start = run;
        //             break 'outer;
        //         } else if idx == end_idx {
        //             return None;
        //         }
        //         last_idx = idx;
        //     }
        //
        //     for (idx, run) in self.gtree.leaves::<false>(last_idx) {
        //         if !run.is_deleted {
        //             start = run;
        //             break 'outer;
        //         } else if idx == end_idx {
        //             return None;
        //         }
        //     }
        //
        //     unreachable!();
        // }
        //
        // if start.contains(deletion.end()) {
        //     let run = start;
        //     let start = deletion.start().offset - run.start();
        //     let end = deletion.end().offset - run.start();
        //     let range = (start..end).into();
        //     self.delete_leaf_range(start_idx, range);
        //     return Some(MergedDeletion::Contiguous(
        //         (range + start_offset).into(),
        //     ));
        // }
        //
        // let start_deletion = start_offset
        //     + (
        //         // TODO: this doesn't work if `start` is deleted or if start
        //         // has been modified
        //         deletion.start().offset - start.start()
        //     );
        //
        // let siblings = self.gtree.siblings::<true>(start_idx);
        //
        // let (stopped_at_idx, len_skipped, len_deleted) =
        //     self.stuff(siblings, start_idx, end_idx, deletion.version_map());
        //
        // if stopped_at_idx == end_idx {
        //     let start = start_deletion + len_skipped;
        //
        //     let end = self.gtree.leaf(end_idx);
        //
        //     let end = if deletion.end().offset < end.len() {
        //         let delete_up_to = deletion.end().offset - end.start();
        //         self.delete_leaf_range(end_idx, (0..delete_up_to).into());
        //         start + len_deleted + delete_up_to
        //     } else {
        //         let len = end.len();
        //         self.delete_leaf(end_idx);
        //         start + len_deleted + end.len()
        //     };
        //
        //     return Some(MergedDeletion::Contiguous(start..end));
        // }

        // TODO: the `delete_leaf_range` logic should be handled by `stuff`
        // (which is already deleting whole leaves).

        // match self.stuff(siblings, start_idx, end_idx, deletion.version_map())
        // {
        //     Stuff::Dont(leaf_idx, len_deleted, delete_up_to) => {
        //         let end = self.gtree.leaf(leaf_idx);
        //         self.delete_leaf_range(end_idx, (0..delete_up_to).into());
        //         let end_deletion = start_deletion + len_deleted + delete_up_to;
        //         let deleted_range = start_deletion..end_deletion;
        //         let mut ranges = vec![deleted_range];
        //
        //         let siblings = self.gtree.siblings::<false>(leaf_idx);
        //
        //         match self.stuff(
        //             siblings,
        //             start_idx,
        //             end_idx,
        //             deletion.version_map(),
        //         ) {
        //             _ => todo!(),
        //         }
        //     },
        //
        //     Stuff::Done(len_deleted) => {
        //         let end = self.gtree.leaf(end_idx);
        //         let delete_up_to = deletion.end().offset - end.start();
        //         self.delete_leaf_range(end_idx, (0..delete_up_to).into());
        //         let end_deletion = start_deletion + len_deleted + delete_up_to;
        //         let deleted_range = start_deletion..end_deletion;
        //         return Some(MergedDeletion::Contiguous(deleted_range));
        //     },
        //
        //     Stuff::Finish(last_idx, len_deleted_in_siblings) => {
        //         let leaves = self.gtree.leaves::<false>(last_idx);
        //
        //         match self.stuff(
        //             leaves,
        //             start_idx,
        //             end_idx,
        //             deletion.version_map(),
        //         ) {
        //             _ => todo!(),
        //         }
        //     },
        // }

        // when we use an iterator one of 3 things can happen:
        //
        // a) we find a run that shouldn't be deleted -> return and switch to
        // split or push a new split to the array
        //
        // -> return the idx of that run and the offset at which we should
        // delete
        //
        // b) we get to the end of the iterator (like for siblings)
        //
        // -> return the offset of the deletion wrt/ the start
        //
        // c) we find the end idx
        //
        // -> return the offset of the deletion wrt/ the start
        //
        // every time we should return
    }

    #[inline]
    pub fn merge_insertion(&mut self, insertion: &Insertion) -> Length {
        let run = EditRun::from_insertion(insertion);

        if run.anchor().is_zero() {
            return self.insert_run_at_origin(run);
        }

        let anchor_idx = self
            .run_indices
            .idx_at_anchor(run.anchor(), insertion.anchor_ts());

        let anchor = self.gtree.leaf(anchor_idx);

        // If the insertion is anchored in the middle of the anchor run then
        // there can't be any other runs that are tied with it. In this case we
        // can just split the anchor run and insert the new run after it.
        if run.anchor().offset < anchor.end() {
            let insert_at = run.anchor().offset - anchor.start();
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
        let (gtree, idx) = Gtree::new(first_run);
        let mut run_indices = RunIndices::new();
        run_indices.get_mut(id).append(len, idx);
        Self { gtree, run_indices }
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

// a)

enum DeletionState {
    Began(Length),

    Done(Option<MergedDeletion>),
}

enum Stuff {
    Dont(LeafIdx<EditRun>, Length, Length),
    Done(Length),
    Finish(LeafIdx<EditRun>, Length),
}

// we start at a leaf_idx
//
// if we first delete then we stop at the first leaf

enum Outcome {
    IteratorEndedWhileDeleting(LeafIdx<EditRun>, Length),

    IteratorEndedWhileSkipping(LeafIdx<EditRun>, Length),

    SkippedLeaves(LeafIdx<EditRun>, Length),

    DeletedLeaves(LeafIdx<EditRun>, Length),

    FoundEnd(LeafIdx<EditRun>, Length),
}

/// TODO: docs
#[derive(Debug)]
pub(crate) enum MergedDeletion {
    Contiguous(ops::Range<Length>),
    Split(Vec<ops::Range<Length>>),
}

/// TODO: docs
#[derive(Clone, PartialEq, Eq)]
#[cfg_attr(feature = "encode", derive(serde::Serialize, serde::Deserialize))]
pub(crate) struct EditRun {
    /// TODO: docs
    inserted_at: Anchor,

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
            "{:?} |@ {:?} - L({}) I({}){}",
            self.text,
            self.inserted_at,
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

enum DeletionRun {
    Starting,
    Deleting(Length),
    Skipping(Length),
}

impl EditRun {
    #[inline(always)]
    pub fn anchor(&self) -> Anchor {
        self.inserted_at
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

    #[inline]
    fn contains(&self, anchor: Anchor) -> bool {
        self.replica_id() == anchor.replica_id()
            && self.text.temporal_range().contains(&anchor.offset)
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
            inserted_at: *insertion.anchor(),
            text: insertion.text().clone(),
            run_ts: insertion.run_ts(),
            lamport_ts: insertion.lamport_ts(),
            is_deleted: false,
        }
    }

    #[inline(always)]
    pub fn run_ts(&self) -> RunTs {
        self.run_ts
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
        run_ts: RunTs,
        lamport_ts: LamportTs,
    ) -> Self {
        Self { inserted_at, text, run_ts, lamport_ts, is_deleted: false }
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

    #[inline]
    fn visible_len(&self) -> Length {
        self.len() * (!self.is_deleted as Length)
    }
}

/// TODO: docs
#[derive(Copy, Clone, PartialEq, Eq)]
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
    #[inline(always)]
    pub fn character_ts(&self) -> Length {
        self.offset
    }

    #[inline(always)]
    pub fn is_zero(&self) -> bool {
        self == &Self::origin()
    }

    #[inline(always)]
    pub fn new(replica_id: ReplicaId, offset: Length) -> Self {
        Self { replica_id, offset }
    }

    #[inline(always)]
    pub fn offset(&self) -> Length {
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
        self.is_deleted = true;
    }
}

impl gtree::Leaf for EditRun {
    type Length = Length;

    #[inline]
    fn len(&self) -> Self::Length {
        self.visible_len()
    }
}

pub(crate) type DebugAsBtree<'a> =
    gtree::DebugAsBtree<'a, RUN_TREE_ARITY, EditRun>;

pub(crate) type DebugAsSelf<'a> =
    gtree::DebugAsSelf<'a, RUN_TREE_ARITY, EditRun>;
