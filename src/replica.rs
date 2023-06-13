use alloc::borrow::Cow;
use alloc::collections::VecDeque;
use core::marker::PhantomData;
use core::ops::RangeBounds;

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
    local_clock: LocalClock,

    /// TODO: docs
    lamport_clock: LamportClock,

    /// TODO: docs
    pending: VecDeque<CrdtEdit>,

    metric: PhantomData<M>,
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
            local_clock: LocalClock::new(),
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

    /// TODO: docs
    #[inline]
    pub fn deleted<R>(&mut self, byte_range: R) -> CrdtEdit
    where
        R: RangeBounds<Length>,
    {
        let (start, end) =
            range_bounds_to_start_end(byte_range, 0, self.len());

        if start == end {
            return CrdtEdit::noop();
        }

        //upstream::delete(
        //    &mut self.insertion_runs,
        //    &mut self.run_indexes,
        //    start..end,
        //);

        CrdtEdit::noop()
    }

    /// TODO: docs
    #[inline]
    pub fn new(len: Length) -> Self {
        let replica_id = ReplicaId::new();

        let mut local_clock = LocalClock::new();

        let mut lamport_clock = LamportClock::new();

        let insertion_id = InsertionId::new(replica_id, local_clock.next());

        let origin_run = InsertionRun::origin(
            insertion_id.clone(),
            lamport_clock.next(),
            len,
        );

        // let run_pointers =
        //     RunIdRegistry::new(insertion_id, origin_run.run_id.clone(), len);

        let insertion_runs = Gtree::new(origin_run);

        Self {
            id: replica_id,
            insertion_runs,
            // run_indexes: run_pointers,
            local_clock,
            lamport_clock,
            pending: VecDeque::new(),
            metric: PhantomData,
        }
    }

    /// TODO: docs
    #[inline]
    pub fn inserted<L, T>(&mut self, offset: L, text: T) -> CrdtEdit
    where
        L: Into<Length>,
        T: Into<String>,
    {
        let text = text.into();

        if text.is_empty() {
            return CrdtEdit::noop();
        }

        let len = M::len(&text);

        //let anchor = upstream::insert(
        //    &mut self.insertion_runs,
        //    &mut self.run_indexes,
        //    &mut self.local_clock,
        //    &mut self.lamport_clock,
        //    self.id,
        //    offset.into(),
        //    len,
        //);

        CrdtEdit::noop()
        // CrdtEdit::insertion(text, id, anchor, lamport_ts)
    }

    /// TODO: docs
    #[allow(clippy::len_without_is_empty)]
    #[doc(hidden)]
    pub fn len(&self) -> Length {
        self.insertion_runs.len()
    }

    /// TODO: docs
    #[inline]
    fn next_insertion_id(&mut self) -> InsertionId {
        InsertionId::new(self.id, self.local_clock.next())
    }

    /// TODO: docs
    #[inline]
    pub fn merge<'a, E>(&mut self, crdt_edit: E) -> Option<TextEdit<'a>>
    where
        E: Into<Cow<'a, CrdtEdit>>,
    {
        let crdt_edit = crdt_edit.into();

        match crdt_edit {
            Cow::Owned(CrdtEdit {
                kind:
                    CrdtEditKind::Insertion { content, id, anchor, lamport_ts },
            }) => self.merge_insertion(
                Cow::Owned(content),
                id,
                anchor,
                lamport_ts,
            ),

            Cow::Borrowed(CrdtEdit {
                kind:
                    CrdtEditKind::Insertion { content, id, anchor, lamport_ts },
            }) => self.merge_insertion(
                Cow::Borrowed(content.as_str()),
                id.clone(),
                anchor.clone(),
                *lamport_ts,
            ),

            _ => None,
        }
    }

    fn merge_insertion<'a>(
        &mut self,
        content: Cow<'a, str>,
        id: InsertionId,
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

impl<M: Metric> From<&str> for Replica<M> {
    #[inline]
    fn from(s: &str) -> Self {
        Self::new(M::len(s))
    }
}

impl From<String> for Replica {
    #[inline]
    fn from(s: String) -> Self {
        s.as_str().into()
    }
}

impl From<alloc::borrow::Cow<'_, str>> for Replica {
    #[inline]
    fn from(moo: alloc::borrow::Cow<'_, str>) -> Self {
        moo.as_ref().into()
    }
}

impl<'a, M: Metric> FromIterator<&'a str> for Replica<M> {
    #[inline]
    fn from_iter<I: IntoIterator<Item = &'a str>>(iter: I) -> Self {
        Self::new(iter.into_iter().map(M::len).sum::<Length>())
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
                .field("local", &self.0.local_clock)
                .field("lamport", &self.0.lamport_clock)
                .field("pending", &self.0.pending)
                .finish()
        }
    }
}
