use alloc::borrow::Cow;
use alloc::collections::VecDeque;
use core::marker::PhantomData;
use core::ops::{Range, RangeBounds};

use uuid::Uuid;

use crate::*;

/// TODO: docs
const ARITY: usize = 32;

/// TODO: docs
pub struct Replica<M: Metric = ByteMetric> {
    /// TODO: docs
    id: ReplicaId,

    /// TODO: docs
    insertion_runs: Gtree<ARITY, InsertionRun>,

    /// TODO: docs
    // run_indexes: RunIdRegistry,

    /// TODO: docs
    character_ts: CharacterTimestamp,

    /// TODO: docs
    lamport_clock: LamportClock,

    /// TODO: docs
    pending: VecDeque<CrdtEdit>,

    metric: PhantomData<M>,
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

/// TODO: docs
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ReplicaId(Uuid);

impl core::fmt::Debug for ReplicaId {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "ReplicaId({:x})", self.as_u32())
    }
}

impl ReplicaId {
    /// TODO: docs
    pub fn as_u32(&self) -> u32 {
        self.0.as_fields().0
    }

    /// Creates a new, randomly generated [`ReplicaId`].
    #[inline(always)]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Returns the "nil" id, i.e. the id whose bytes are all zeros.
    ///
    /// This is used to form the [`EditId`] of the first edit run and should
    /// never be used in any of the following user-generated insertion.
    #[inline(always)]
    pub const fn zero() -> Self {
        Self(Uuid::nil())
    }
}

impl<M: Metric> Clone for Replica<M> {
    #[inline(always)]
    fn clone(&self) -> Self {
        let mut lamport_clock = self.lamport_clock;

        lamport_clock.next();

        Self {
            id: ReplicaId::new(),
            insertion_runs: self.insertion_runs.clone(),
            character_ts: 0,
            // run_indexes: self.run_indexes.clone(),
            lamport_clock,
            pending: self.pending.clone(),
            metric: PhantomData,
        }
    }
}

impl<M: Metric> Replica<M> {
    #[cfg(debug_assertions)]
    #[doc(hidden)]
    pub fn debug(&self) -> debug::Debug<'_, M> {
        debug::Debug(self)
    }

    #[cfg(debug_assertions)]
    #[doc(hidden)]
    pub fn debug_as_btree(&self) -> debug::DebugAsBtree<'_, M> {
        debug::DebugAsBtree(self)
    }

    /// TODO: docs
    #[inline]
    pub fn deleted<R>(&mut self, range: R) -> CrdtEdit
    where
        R: RangeBounds<Length>,
    {
        let (start, end) = range_bounds_to_start_end(range, 0, self.len());

        if start == end {
            return CrdtEdit::no_op();
        }

        //let delete_leaf = InsertionRun::delete;

        //let delete_from = InsertionRun::delete_from;

        //let delete_up_to = InsertionRun::delete_up_to;

        //let delete_range = InsertionRun::delete_range;

        //self.insertion_runs.delete_range(
        //    start..end,
        //    delete_leaf,
        //    delete_from,
        //    delete_up_to,
        //    delete_range,
        //);

        CrdtEdit::no_op()
    }

    /// TODO: docs
    #[inline]
    pub fn new(len: u64) -> Self {
        let replica_id = ReplicaId::new();

        let mut lamport_clock = LamportClock::new();

        let origin_run = InsertionRun::new(
            Anchor::origin(),
            replica_id,
            0..len,
            lamport_clock.next(),
        );

        // let run_pointers =
        //     RunIdRegistry::new(insertion_id, origin_run.run_id.clone(), len);

        let insertion_runs = Gtree::new(origin_run);

        Self {
            id: replica_id,
            insertion_runs,
            // run_indexes: run_pointers,
            character_ts: len,
            lamport_clock,
            pending: VecDeque::new(),
            metric: PhantomData,
        }
    }

    /// TODO: docs
    #[inline]
    pub fn inserted(&mut self, offset: Length, len: Length) -> CrdtEdit {
        if len == 0 {
            return CrdtEdit::no_op();
        }

        let len = len as u64;

        let mut edit = CrdtEdit::no_op();

        let insert_with = |run: &mut InsertionRun, offset: u64| {
            if offset == run.len()
                && self.id == run.replica_id()
                && self.character_ts == run.end()
            {
                edit = CrdtEdit::insertion(
                    Anchor::new(run.replica_id(), run.end()),
                    self.id,
                    len,
                    run.lamport_ts(),
                );

                run.extend(len);

                return (None, None);
            }

            let range = self.character_ts..self.character_ts + len;

            let lamport_ts = self.lamport_clock.next();

            if offset == 0 {
                let run = InsertionRun::new(
                    Anchor::origin(),
                    self.id,
                    range,
                    lamport_ts,
                );

                edit = CrdtEdit::insertion(
                    Anchor::origin(),
                    self.id,
                    len,
                    lamport_ts,
                );

                (Some(run), None)
            } else {
                let split = run.split(offset);

                let anchor = Anchor::new(run.replica_id(), run.end());

                edit = CrdtEdit::insertion(
                    anchor.clone(),
                    self.id,
                    len,
                    lamport_ts,
                );

                let new_run =
                    InsertionRun::new(anchor, self.id, range, lamport_ts);

                (Some(new_run), split)
            }
        };

        self.insertion_runs.insert_at_offset(offset as u64, insert_with);

        edit
    }

    fn next_range(&mut self, inserted_len: u64) -> Range<CharacterTimestamp> {
        let start = self.character_ts;
        self.character_ts += inserted_len;
        start..self.character_ts
    }

    /// TODO: docs
    #[allow(clippy::len_without_is_empty)]
    #[doc(hidden)]
    pub fn len(&self) -> Length {
        self.insertion_runs.summary() as _
    }

    /// TODO: docs
    #[inline]
    pub fn merge<'a, E>(&mut self, crdt_edit: E) -> Option<TextEdit<'a>>
    where
        E: Into<Cow<'a, CrdtEdit>>,
    {
        let crdt_edit = crdt_edit.into();

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

    fn merge_insertion<'a>(
        &mut self,
        content: Cow<'a, str>,
        anchor: Anchor,
        lamport_ts: LamportTimestamp,
    ) -> Option<TextEdit<'a>> {
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

#[cfg(debug_assertions)]
mod debug {
    use super::*;

    pub struct Debug<'a, M: Metric>(pub &'a Replica<M>);

    impl<'a, M: Metric> core::fmt::Debug for Debug<'a, M> {
        fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
            f.debug_struct("Replica")
                .field("id", &self.0.id)
                .field("edit_runs", &self.0.insertion_runs)
                // .field("id_registry", &self.0.run_indexes)
                .field("character", &self.0.character_ts)
                .field("lamport", &self.0.lamport_clock)
                .field("pending", &self.0.pending)
                .finish()
        }
    }

    pub struct DebugAsBtree<'a, M: Metric>(pub &'a Replica<M>);

    impl<'a, M: Metric> core::fmt::Debug for DebugAsBtree<'a, M> {
        fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
            f.debug_struct("Replica")
                .field("id", &self.0.id)
                .field("edit_runs", &self.0.insertion_runs.debug_as_btree())
                // .field("id_registry", &self.0.run_indexes)
                .field("character", &self.0.character_ts)
                .field("lamport", &self.0.lamport_clock)
                .field("pending", &self.0.pending)
                .finish()
        }
    }
}
