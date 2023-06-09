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

/// TODO: docs
#[derive(Clone)]
pub struct Replica {
    /// TODO: docs
    id: ReplicaId,

    /// TODO: docs
    edit_runs: Btree<ARITY, EditRun>,

    /// TODO: docs
    run_pointers: RunIdRegistry,

    /// TODO: docs
    local_clock: LocalClock,

    /// TODO: docs
    lamport_clock: LamportClock,

    /// TODO: docs
    pending: VecDeque<CrdtEdit>,
}

impl Replica {
    pub(crate) const fn arity() -> usize {
        ARITY
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
            run_pointers,
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

        let edit_id = self.next_edit_id();

        let lamport_ts = self.lamport_clock.next();

        let insertion_id = upstream_insert::inserted(
            &mut self.edit_runs,
            edit_id,
            lamport_ts,
            byte_offset,
            text.len(),
        );

        CrdtEdit::insertion(text, insertion_id, lamport_ts)
    }

    /// TODO: docs
    #[inline]
    pub fn len(&self) -> usize {
        self.edit_runs.summary().len
    }

    /// TODO: docs
    #[inline]
    fn next_edit_id(&mut self) -> InsertionId {
        InsertionId::new(self.id, self.local_clock.next())
    }

    /// TODO: docs
    #[inline]
    pub fn merge(&mut self, crdt_edit: &CrdtEdit) -> Option<TextEdit> {
        None
    }

    /// TODO: docs
    #[inline]
    pub fn new() -> Self {
        Self::from_chunks(core::iter::empty())
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
                .field("local", &self.0.local_clock)
                .field("lamport", &self.0.lamport_clock)
                .field("pending", &self.0.pending)
                .finish()
        }
    }
}

type Node = node::Node<ARITY, EditRun>;

type Inode = node::Inode<ARITY, EditRun>;

mod upstream_insert {
    //! This module handles the logic used to create [`CrdtEdit`]s after the
    //! user locally inserts some text in their buffer.

    use super::*;

    type Btree = super::Btree<ARITY, EditRun>;

    /// TODO: docs
    pub fn inserted(
        btree: &mut Btree,
        edit_id: InsertionId,
        lamport_ts: LamportTimestamp,
        byte_offset: usize,
        text_len: usize,
    ) -> InsertionAnchor {
        let root = match btree.root_mut() {
            Node::Internal(inode) => inode,

            Node::Leaf(edit_run) => {
                let (new_run, split_run) = edit_run.insert(
                    edit_id,
                    lamport_ts,
                    byte_offset,
                    text_len,
                );

                let insertion_id = new_run.insertion_id();

                let new_run = Node::Leaf(new_run);

                btree.replace_root(|first_run| {
                    let new_children =
                        if let Some(split_run) = split_run.map(Node::Leaf) {
                            vec![first_run, new_run, split_run]
                        } else {
                            vec![first_run, new_run]
                        };

                    Node::from_children(new_children)
                });

                return insertion_id;
            },
        };

        let (insertion_id, root_split) =
            insert(root, edit_id, lamport_ts, byte_offset, text_len);

        if let Some(root_split) = root_split.map(Node::Internal) {
            btree.replace_root(|old_root| {
                Node::from_children(vec![old_root, root_split])
            });
        }

        insertion_id
    }

    /// TODO: docs
    fn insert(
        inode: &mut Inode,
        edit_id: InsertionId,
        lamport_ts: LamportTimestamp,
        byte_offset: usize,
        text_len: usize,
    ) -> (InsertionAnchor, Option<Inode>) {
        let mut offset = 0;

        for (idx, child) in inode.children_mut().iter_mut().enumerate() {
            let child_len = child.summary().len;

            offset += child_len;

            if offset >= byte_offset {
                offset -= child_len;

                if child.is_internal() {
                    let (id, split) = inode.with_child_mut(idx, |child| {
                        insert(
                            child.as_internal_mut(),
                            edit_id,
                            lamport_ts,
                            byte_offset - offset,
                            text_len,
                        )
                    });

                    let split = split.and_then(|s| {
                        inode.insert(idx + 1, Node::Internal(s))
                    });

                    return (id, split);
                } else {
                    let (new_run, split_run) =
                        inode.with_child_mut(idx, |child| {
                            child.as_leaf_mut().insert(
                                edit_id,
                                lamport_ts,
                                byte_offset - offset,
                                text_len,
                            )
                        });

                    let id = new_run.insertion_id();

                    let new_run = Node::Leaf(new_run);

                    let offset = idx + 1;

                    let split = if let Some(split_run) =
                        split_run.map(Node::Leaf)
                    {
                        inode.insert_two(offset, new_run, offset, split_run)
                    } else {
                        inode.insert(offset, new_run)
                    };

                    return (id, split);
                }
            }
        }

        unreachable!()
    }
}

mod upstream_delete {
    //! This module handles the logic used to create [`CrdtEdit`]s after the
    //! user locally deletes some text in their buffer.

    use super::*;
}
