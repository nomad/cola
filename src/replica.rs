use alloc::borrow::Cow;
use alloc::collections::VecDeque;
use core::ops::RangeBounds;

use uuid::Uuid;

use crate::*;

/// TODO: docs
const ARITY: usize = 4;

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
    fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Returns the "nil" id, i.e. the id whose bytes are all zeros.
    ///
    /// This is used to form the [`EditId`] of the first edit run and should
    /// never be used in any of the following user-generated insertion.
    pub const fn zero() -> Self {
        Self(Uuid::nil())
    }
}

pub type RunTree = Btree<ARITY, EditRun>;
pub type RunNode = Node<ARITY, EditRun>;
pub type RunInode = Inode<ARITY, EditRun>;

/// TODO: docs
#[derive(Clone)]
pub struct Replica {
    /// TODO: docs
    id: ReplicaId,

    /// TODO: docs
    edit_runs: RunTree,

    /// TODO: docs
    id_registry: RunIdRegistry,

    /// TODO: docs
    local_clock: LocalClock,

    /// TODO: docs
    lamport_clock: LamportClock,

    /// TODO: docs
    pending: VecDeque<CrdtEdit>,
}

impl Replica {
    #[cfg(debug_assertions)]
    #[doc(hidden)]
    pub fn debug(&self) -> debug::Debug<'_> {
        debug::Debug(self)
    }

    /// TODO: docs
    #[inline]
    pub fn deleted<R>(&mut self, byte_range: R) -> CrdtEdit
    where
        R: RangeBounds<usize>,
    {
        let (start, end) =
            range_bounds_to_start_end(byte_range, 0, self.len());

        upstream::delete(
            &mut self.edit_runs,
            &mut self.id_registry,
            start..end,
        );

        CrdtEdit::noop()
    }

    /// TODO: docs
    #[inline]
    pub fn from_chunks<'a, Chunks>(chunks: Chunks) -> Self
    where
        Chunks: Iterator<Item = &'a str>,
    {
        let replica_id = ReplicaId::new();

        let mut local_clock = LocalClock::default();

        let mut lamport_clock = LamportClock::default();

        let insertion_id = InsertionId::new(replica_id, local_clock.next());

        let len = chunks.map(|s| s.len()).sum::<usize>();

        let origin_run =
            EditRun::origin(insertion_id, lamport_clock.next(), len);

        let run_pointers =
            RunIdRegistry::new(insertion_id, origin_run.run_id().clone(), len);

        let edit_runs = Btree::from(origin_run);

        Self {
            id: replica_id,
            edit_runs,
            id_registry: run_pointers,
            local_clock,
            lamport_clock,
            pending: VecDeque::new(),
        }
    }

    /// TODO: docs
    #[inline]
    pub fn inserted<T>(&mut self, byte_offset: usize, text: T) -> CrdtEdit
    where
        T: Into<String>,
    {
        let text = text.into();

        let id = self.next_insertion_id();

        let lamport_ts = self.lamport_clock.next();

        let anchor = upstream::insert(
            &mut self.edit_runs,
            &mut self.id_registry,
            id,
            lamport_ts,
            byte_offset,
            text.len(),
        );

        CrdtEdit::insertion(text, id, anchor, lamport_ts)
    }

    /// TODO: docs
    #[allow(clippy::len_without_is_empty)]
    #[doc(hidden)]
    pub fn len(&self) -> usize {
        self.edit_runs.summary().len
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
                *id,
                *anchor,
                *lamport_ts,
            ),

            _ => None,
        }
    }

    fn merge_insertion<'a>(
        &mut self,
        content: Cow<'a, str>,
        id: InsertionId,
        anchor: InsertionAnchor,
        lamport_ts: LamportTimestamp,
    ) -> Option<TextEdit<'a>> {
        let Some(run_id) = self.id_registry.get_run_id(&anchor) else {
            let crdt_edit = CrdtEdit::insertion(
                content.into_owned(),
                id,
                anchor,
                lamport_ts,
            );
            self.pending.push_back(crdt_edit);
            return None;
        };

        let lamport_ts = self.lamport_clock.update(lamport_ts);

        let offset = downstream::insert(
            &mut self.edit_runs,
            &mut self.id_registry,
            id,
            lamport_ts,
            anchor,
            run_id,
            content.len(),
        );

        Some(TextEdit::new(content, offset..offset))
    }

    /// TODO: docs
    #[inline]
    pub fn new() -> Self {
        Self::from_chunks(core::iter::empty())
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
        Self::new()
    }
}

impl From<&str> for Replica {
    #[inline]
    fn from(s: &str) -> Self {
        Self::from_chunks(core::iter::once(s))
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

impl<'a> FromIterator<&'a str> for Replica {
    #[inline]
    fn from_iter<I: IntoIterator<Item = &'a str>>(iter: I) -> Self {
        Self::from_chunks(iter.into_iter())
    }
}

#[inline]
fn range_bounds_to_start_end<R>(
    range: R,
    lo: usize,
    hi: usize,
) -> (usize, usize)
where
    R: core::ops::RangeBounds<usize>,
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

#[cfg(debug_assertions)]
mod debug {
    use super::*;

    pub struct Debug<'a>(pub &'a Replica);

    impl<'a> core::fmt::Debug for Debug<'a> {
        fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
            f.debug_struct("Replica")
                .field("id", &self.0.id)
                .field("edit_runs", &self.0.edit_runs)
                .field("id_registry", &self.0.id_registry)
                .field("local", &self.0.local_clock)
                .field("lamport", &self.0.lamport_clock)
                .field("pending", &self.0.pending)
                .finish()
        }
    }
}
