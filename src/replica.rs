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
    id_registry: RunIdRegistry,

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

        upstream_delete::deleted(
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

        let anchor = upstream_insert::inserted(
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
    #[inline]
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
                .field("id_registry", &self.0.id_registry)
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
        id_registry: &mut RunIdRegistry,
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
                    id_registry,
                );

                let anchor = new_run.anchor();

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

                return anchor;
            },
        };

        let (insertion_id, root_split) = insert(
            root,
            id_registry,
            edit_id,
            lamport_ts,
            byte_offset,
            text_len,
        );

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
        id_registry: &mut RunIdRegistry,
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
                            id_registry,
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
                            let edit_run = child.as_leaf_mut();

                            edit_run.insert(
                                edit_id,
                                lamport_ts,
                                byte_offset - offset,
                                text_len,
                                id_registry,
                            )
                        });

                    let anchor = new_run.anchor();

                    let new_run = Node::Leaf(new_run);

                    let offset = idx + 1;

                    let split = if let Some(split_run) =
                        split_run.map(Node::Leaf)
                    {
                        inode.insert_two(offset, new_run, offset, split_run)
                    } else {
                        inode.insert(offset, new_run)
                    };

                    return (anchor, split);
                }
            }
        }

        unreachable!()
    }
}

mod upstream_delete {
    //! This module handles the logic used to create [`CrdtEdit`]s after the
    //! user locally deletes some text in their buffer.

    use core::ops::Range;

    use super::*;

    type Btree = super::Btree<ARITY, EditRun>;

    /// TODO: docs
    pub fn deleted(
        btree: &mut Btree,
        id_registry: &mut RunIdRegistry,
        byte_range: Range<usize>,
    ) {
        let root = match btree.root_mut() {
            Node::Internal(inode) => inode,

            Node::Leaf(edit_run) => {
                let (deleted, rest) =
                    edit_run.delete_range(byte_range, id_registry);

                if let Some(deleted) = deleted.map(Node::Leaf) {
                    btree.replace_root(|old_root| {
                        let children = if let Some(rest) = rest.map(Node::Leaf)
                        {
                            vec![old_root, deleted, rest]
                        } else {
                            vec![old_root, deleted]
                        };

                        Node::from_children(children)
                    })
                }

                return;
            },
        };

        if let Some(root_split) =
            delete(root, id_registry, byte_range).map(Node::Internal)
        {
            btree.replace_root(|old_root| {
                Node::from_children(vec![old_root, root_split])
            });
        }
    }

    fn delete(
        inode: &mut Inode,
        id_registry: &mut RunIdRegistry,
        mut byte_range: Range<usize>,
    ) -> Option<Inode> {
        let mut offset = 0;

        for (idx, child) in inode.children().iter().enumerate() {
            let child_len = child.summary().len;

            offset += child_len;

            if offset < byte_range.start {
                continue;
            }

            if offset >= byte_range.end {
                offset -= child_len;

                byte_range.start -= offset;
                byte_range.end -= offset;

                if child.is_internal() {
                    let split = inode.with_child_mut(idx, |child| {
                        let child = child.as_internal_mut();
                        delete(child, id_registry, byte_range)
                    });

                    return split.and_then(|s| {
                        inode.insert(idx + 1, Node::Internal(s))
                    });
                } else {
                    let (deleted, rest) = inode.with_child_mut(idx, |child| {
                        let edit_run = child.as_leaf_mut();
                        edit_run.delete_range(byte_range, id_registry)
                    });

                    let deleted = deleted.map(Node::Leaf)?;

                    let offset = idx + 1;

                    return if let Some(rest) = rest.map(Node::Leaf) {
                        inode.insert_two(offset, deleted, offset, rest)
                    } else {
                        inode.insert(offset, deleted)
                    };
                }
            } else {
                return delete_range_in_deepest(
                    inode,
                    id_registry,
                    byte_range,
                );
            }
        }

        unreachable!();
    }

    fn delete_range_in_deepest(
        inode: &mut Inode,
        id_registry: &mut RunIdRegistry,
        delete_range: Range<usize>,
    ) -> Option<Inode> {
        let mut start_idx = 0;

        let mut end_idx = 0;

        let mut extra_from_start = None;

        let mut extra_from_end = None;

        let mut children = inode.children_mut().iter_mut().enumerate();

        let mut offset = 0;

        for (idx, child) in children.by_ref() {
            let child_len = child.summary().len;

            offset += child_len;

            if offset >= delete_range.start {
                start_idx = idx;
                let delete_from = delete_range.start + child_len - offset;
                extra_from_start =
                    something_start(child, id_registry, delete_from);
                break;
            }
        }

        for (idx, child) in children {
            let child_len = child.summary().len;

            offset += child_len;

            if offset >= delete_range.end {
                end_idx = idx;
                let delete_up_to = delete_range.end + child_len - offset;
                extra_from_end =
                    something_end(child, id_registry, delete_up_to);
                break;
            } else {
                delete_node(child);
            }
        }

        *inode.summary_mut() = inode.summarize();

        match (extra_from_start, extra_from_end) {
            (Some(start), Some(end)) => {
                inode.insert_two(start_idx + 1, start, end_idx + 1, end)
            },

            (Some(start), None) => inode.insert(start_idx + 1, start),

            (None, Some(end)) => inode.insert(end_idx + 1, end),

            (None, None) => None,
        }
    }

    fn something_start(
        node: &mut Node,
        id_registry: &mut RunIdRegistry,
        delete_from: usize,
    ) -> Option<Node> {
        let inode = match node {
            Node::Internal(inode) => inode,

            Node::Leaf(edit_run) => {
                return edit_run
                    .delete_from(delete_from, id_registry)
                    .map(Node::Leaf);
            },
        };

        let mut start_idx = 0;

        let mut extra = None;

        let mut offset = 0;

        let mut children = inode.children_mut().iter_mut();

        for (idx, child) in children.by_ref().enumerate() {
            let child_len = child.summary().len;

            if offset + child_len >= delete_from {
                start_idx = idx;
                extra =
                    something_start(child, id_registry, delete_from - offset);
                break;
            } else {
                offset += child_len;
            }
        }

        for child in children {
            delete_node(child);
        }

        *inode.summary_mut() = inode.summarize();

        extra.and_then(|e| inode.insert(start_idx + 1, e)).map(Node::Internal)
    }

    fn something_end(
        node: &mut Node,
        id_registry: &mut RunIdRegistry,
        delete_up_to: usize,
    ) -> Option<Node> {
        let inode = match node {
            Node::Internal(inode) => inode,

            Node::Leaf(edit_run) => {
                return edit_run
                    .delete_up_to(delete_up_to, id_registry)
                    .map(Node::Leaf);
            },
        };

        let mut end_idx = 0;

        let mut extra = None;

        let mut offset = 0;

        for (idx, child) in inode.children_mut().iter_mut().enumerate() {
            let child_len = child.summary().len;

            if offset + child_len >= delete_up_to {
                end_idx = idx;
                extra =
                    something_end(child, id_registry, delete_up_to - offset);
                break;
            } else {
                delete_node(child);
                offset += child_len;
            }
        }

        *inode.summary_mut() = inode.summarize();

        extra.and_then(|e| inode.insert(end_idx + 1, e)).map(Node::Internal)
    }

    fn delete_node(node: &mut Node) {
        match node {
            Node::Internal(inode) => {
                inode.summary_mut().len = 0;
            },

            Node::Leaf(edit_run) => edit_run.delete(),
        }
    }
}
