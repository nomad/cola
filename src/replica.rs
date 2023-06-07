use core::ops::RangeBounds;
use std::collections::VecDeque;

use uuid::Uuid;

use crate::*;

const ARITY: usize = 4;

/// TODO: docs
#[derive(Clone)]
pub struct Replica {
    /// TODO: docs
    id: ReplicaId,

    /// TODO: docs
    edit_runs: Btree<ARITY, EditRun>,

    /// TODO: docs
    local_clock: LocalClock,

    /// TODO: docs
    lamport_clock: LamportClock,

    /// TODO: docs
    pending: VecDeque<CrdtEdit>,
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

impl Replica {
    pub(crate) const fn arity() -> usize {
        ARITY
    }

    /// TODO: docs
    #[inline]
    pub fn contents(&self) -> impl Iterator<Item = &str> {
        core::iter::empty()
    }

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

        self.edit_runs.delete(start..end);

        CrdtEdit::noop()
    }

    /// TODO: docs
    #[inline]
    pub fn inserted<T>(&mut self, byte_offset: usize, text: T) -> CrdtEdit
    where
        T: Into<String>,
    {
        let text = text.into();

        let id = self.next_edit_id();

        let lamport = self.lamport_clock.next();

        let mut fragment = EditRun::default();

        let insert_at = ByteMetric(byte_offset);

        self.edit_runs.insert(insert_at, |ByteMetric(offset), parent| {
            let len = text.len();

            let run_id: RunId = todo!();

            if byte_offset == 0 {
                // The parent is the edit that we're inserting *after*. When
                // the user inserts at the beginning of the buffer there's no
                // such edit.
                // In this case we use a special `EditId` called "zero" whose
                // replica id and timestamp are both 0.
                let edit =
                    EditRun::new(id, run_id, EditId::zero(), 0, lamport, len);
                fragment = edit;
                return (core::mem::replace(parent, edit), None);
            }

            let parent_offset = byte_offset - offset;

            let edit = EditRun::new(
                id,
                run_id,
                parent.id(),
                parent_offset,
                lamport,
                len,
            );

            fragment = edit;

            (edit, parent.split(parent_offset))
        });

        CrdtEdit::insertion(fragment, text)
    }

    /// TODO: docs
    #[inline]
    pub fn len(&self) -> usize {
        self.edit_runs.summary().len
    }

    /// TODO: docs
    #[inline]
    pub fn new<'a, Chunks>(chunks: Chunks) -> Self
    where
        Chunks: Iterator<Item = &'a str>,
    {
        let id = ReplicaId::new();
        let mut local_clock = LocalClock::default();
        let mut lamport_clock = LamportClock::default();

        let edit = EditId::new(id, local_clock.next());

        let origin = EditId::zero();

        let lamport = lamport_clock.next();

        let len = chunks.map(|s| s.len()).sum::<usize>();

        let fragment =
            EditRun::new(edit, RunId::default(), origin, 0, lamport, len);

        let fragment_tree = Btree::from(fragment);

        Self {
            id,
            edit_runs: fragment_tree,
            local_clock,
            lamport_clock,
            pending: VecDeque::new(),
        }
    }

    #[inline]
    fn next_edit_id(&mut self) -> EditId {
        EditId::new(self.id, self.local_clock.next())
    }

    /// TODO: docs
    #[inline]
    pub fn merge(
        &mut self,
        crdt_edit: &CrdtEdit,
    ) -> impl Iterator<Item = TextEdit> {
        core::iter::empty()
    }

    /// TODO: docs
    #[inline]
    pub fn replaced<R, T>(&mut self, byte_range: R, text: T) -> CrdtEdit
    where
        R: RangeBounds<usize>,
        T: Into<String>,
    {
        todo!();
    }

    /// TODO: docs
    #[inline]
    pub fn undo(&self, crdt_edit: &CrdtEdit) -> CrdtEdit {
        todo!();
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

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ReplicaId(Uuid);

impl core::fmt::Debug for ReplicaId {
    #[inline]
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        let id = self.0.as_fields().0;
        write!(f, "ReplicaId({:x})", id)
    }
}

impl Default for ReplicaId {
    #[inline]
    fn default() -> Self {
        Self::zero()
    }
}

impl ReplicaId {
    #[inline]
    fn new() -> Self {
        Self(Uuid::new_v4())
    }

    #[inline]
    pub(super) const fn zero() -> Self {
        Self(Uuid::nil())
    }
}

/// TODO: docs
#[derive(Copy, Clone, Default)]
pub(super) struct EditId {
    /// TODO: docs
    created_by: ReplicaId,

    /// TODO: docs
    local_timestamp_at_creation: LocalTimestamp,
}

impl core::fmt::Debug for EditId {
    #[inline]
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        let id = self.created_by.0.as_fields().0;
        write!(f, "{:x}.{}", id, self.local_timestamp_at_creation.as_u64())
    }
}

impl EditId {
    #[inline]
    fn new(replica_id: ReplicaId, timestamp: LocalTimestamp) -> Self {
        Self { created_by: replica_id, local_timestamp_at_creation: timestamp }
    }

    /// TODO: docs
    #[inline]
    fn zero() -> Self {
        Self::default()
    }
}

impl From<&str> for Replica {
    #[inline]
    fn from(s: &str) -> Self {
        Self::new(core::iter::once(s))
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

#[cfg(debug_assertions)]
mod debug {
    use super::*;

    pub struct Debug<'a>(pub &'a Replica);

    impl<'a> core::fmt::Debug for Debug<'a> {
        fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
            f.debug_struct("Replica")
                .field("id", &self.0.id)
                .field("fragments", &self.0.edit_runs)
                .field("local", &self.0.local_clock)
                .field("lamport", &self.0.lamport_clock)
                .field("pending", &self.0.pending)
                .finish()
        }
    }
}
