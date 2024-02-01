use core::fmt::Debug;
use core::marker::PhantomData;
use core::mem;

use crate::{Length, Range, RangeExt};

trait LengthExt {
    type Diff;

    /// Creates a diff which, when applied to `old_len`, produces `new_len`.
    fn diff(old_len: Self, new_len: Self) -> Self::Diff;

    /// Applies a diff to `self`.
    fn patch(&mut self, diff: Self::Diff);
}

#[derive(Debug, Clone, Copy)]
enum Diff {
    Add(Length),
    Subtract(Length),
}

impl LengthExt for Length {
    type Diff = Diff;

    #[inline]
    fn diff(from: Self, to: Self) -> Diff {
        if from < to {
            Diff::Add(to - from)
        } else {
            Diff::Subtract(from - to)
        }
    }

    #[inline]
    fn patch(&mut self, diff: Diff) {
        match diff {
            Diff::Add(add) => *self += add,
            Diff::Subtract(sub) => *self -= sub,
        }
    }
}

/// A trait used to delete the contents of a leaf node.
///
/// The [`delete()`] method is called on every leaf node that falls inside the
/// range passed to [`Gtree::delete()`].
pub trait Delete {
    fn delete(&mut self);
}

/// A trait used to join two leaf nodes together.
pub trait Join: Sized {
    /// Try to append `other` to the end of `self`.
    ///
    /// The method returns `Ok(())` if the join operation succeeded and
    /// `Err(other)` if it failed.
    #[allow(unused_variables)]
    fn append(&mut self, other: Self) -> Result<(), Self> {
        Err(other)
    }

    /// Try to prepend `other` to the beginning of `self`.
    ///
    /// The method returns `None` if the join operation succeeded and
    /// `Some(other)` if it failed.
    #[allow(unused_variables)]
    fn prepend(&mut self, other: Self) -> Result<(), Self> {
        Err(other)
    }
}

/// A trait to be implemented by the leaf nodes of a Gtree.
pub trait Leaf: Debug + Join {
    fn len(&self) -> Length;

    #[inline]
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Statically checks that `InodeIdx` and `LeafIdx` have identical size and
/// alignment.
///
/// This invariant is required by [`Inode::children()`] to safely transmute
/// `NodeIdx` slices into `InodeIdx` and `LeafIdx` slices.
const _NODE_IDX_LAYOUT_CHECK: usize = {
    use core::alloc::Layout;
    let i = Layout::new::<InodeIdx>();
    let l = Layout::new::<LeafIdx<()>>();
    (i.size() == l.size() && i.align() == l.align()) as usize - 1
};

/// A grow-only, self-balancing tree.
///
/// A Gtree is a tree data structure that has very similar invariants and
/// performance characteristics to those of a Btree, but differs in the way in
/// which the internal and leaf nodes are stored in memory.
///
/// TODO: finish describing the data structure.
#[derive(Clone)]
pub(crate) struct Gtree<const ARITY: usize, L: Leaf> {
    /// The internal nodes of the Gtree.
    ///
    /// The order in which the inodes appear in this vector doesn't have any
    /// particular meaning.
    inodes: Vec<Inode<ARITY, L>>,

    /// The leaf nodes of the Gtree.
    ///
    /// The order in which the lnodes appear in this vector doesn't have any
    /// particular meaning.
    lnodes: Vec<Lnode<L>>,

    /// An index into `self.inodes` which points to the current root of the
    /// Gtree.
    root_idx: InodeIdx,

    /// The position of the last insertion/deletion.
    ///
    /// Saving this allows to make repeated edits at the same cursor position
    /// fast af.
    cursor: Option<Cursor<L>>,
}

/// A newtype struct around the index of an internal node of the Gtree.
///
/// It can be passed to [`Gtree::inode()`] and [`Gtree::inode_mut()`] to
/// get access to the inode.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct InodeIdx(usize);

impl InodeIdx {
    /// Returns a "dangling" index which doesn't point to any inode of the
    /// Gtree.
    ///
    /// This value is used by [`Inode`]s to:
    ///
    /// 1. Initialize their `children` field by filling the array with this
    ///    value;
    ///
    /// 2. For the root node, to indicate that it has no parent.
    #[inline]
    pub(crate) const fn dangling() -> Self {
        Self(usize::MAX)
    }

    /// Returns `true` if this index is dangling.
    ///
    /// See [`Self::dangling()`] for more information.
    #[inline]
    fn is_dangling(self) -> bool {
        self == Self::dangling()
    }
}

/// A stable identifier for a particular leaf in the Gtree.
#[derive(Eq)]
pub struct LeafIdx<L> {
    idx: usize,
    _pd: PhantomData<L>,
}

impl<L> Copy for LeafIdx<L> {}

impl<L> Clone for LeafIdx<L> {
    #[inline]
    fn clone(&self) -> Self {
        *self
    }
}

impl<L> PartialEq<LeafIdx<L>> for LeafIdx<L> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.idx == other.idx
    }
}

impl<L> LeafIdx<L> {
    /// Returns a "dangling" index which doesn't point to any leaf of the
    /// Gtree.
    #[inline]
    pub(crate) const fn dangling() -> Self {
        Self::new(usize::MAX)
    }

    #[cfg(feature = "encode")]
    #[inline]
    pub(crate) fn into_usize(self) -> usize {
        self.idx
    }

    #[inline]
    const fn new(idx: usize) -> Self {
        Self { idx, _pd: PhantomData }
    }
}

/// A cursor into the Gtree.
///
/// It's called a "cursor" because it identifies a particular position between
/// two leaf nodes in the tree, much like a line cursor identifies a position
/// between two characters in a text editor.
#[derive(PartialEq, Eq)]
struct Cursor<L: Leaf> {
    /// The index of the leaf node that comes *after* the cursor. There always
    /// is one because the cursor is never parked after the last leafof the
    /// Gtree.
    leaf_idx: LeafIdx<L>,

    /// The offset of `leaf_idx` in the Gtree, *without* taking into account
    /// the length of the leaf node itself.
    offset: Length,

    /// The child index of `leaf_idx` within its parent.
    child_idx: ChildIdx,
}

impl<L: Leaf> Copy for Cursor<L> {}

impl<L: Leaf> Clone for Cursor<L> {
    #[inline]
    fn clone(&self) -> Self {
        *self
    }
}

impl<L: Leaf> Cursor<L> {
    #[inline]
    fn new(leaf_idx: LeafIdx<L>, offset: Length, child_idx: ChildIdx) -> Self {
        Self { leaf_idx, offset, child_idx }
    }
}

// Public API.
impl<const ARITY: usize, L: Leaf> Gtree<ARITY, L> {
    /// Appends a new leaf node to the end of the Gtree, returning its newly
    /// created leaf index.
    #[inline(always)]
    pub fn append(&mut self, leaf: L) -> LeafIdx<L> {
        let (last_leaf_idx, idx_in_parent) = self.last_leaf();
        self.insert_leaf_after_leaf(last_leaf_idx, idx_in_parent, leaf)
    }

    /// Appends a leaf to the leaf at the given index.
    ///
    /// Returns the offset of the leaf that was appended to relative to the
    /// start of the Gtree.
    #[inline]
    pub fn append_leaf_to_another(
        &mut self,
        append_to: LeafIdx<L>,
        leaf: L,
    ) -> Length {
        let (leaf_offset, idx_in_parent) = self
            .cursor
            .and_then(|cursor| {
                if append_to == cursor.leaf_idx {
                    Some((cursor.offset, cursor.child_idx))
                } else {
                    None
                }
            })
            .unwrap_or_else(|| {
                let offset = self.offset_of_leaf(append_to);
                let idx_in_parent = self.idx_of_leaf_in_parent(append_to);
                (offset, idx_in_parent)
            });

        self.with_leaf_mut(append_to, |append_to| {
            append_to.append(leaf).unwrap()
        });

        self.cursor = Some(Cursor::new(append_to, leaf_offset, idx_in_parent));

        leaf_offset
    }

    /// Asserts the invariants of the Gtree.
    ///
    /// If this method returns without panicking, then the Gtree is in a
    /// consistent state.
    ///
    /// Only used for debugging.
    pub fn assert_invariants(&self) {
        fn recursively_assert_inode_invariants<const N: usize, L: Leaf>(
            gtree: &Gtree<N, L>,
            idx: InodeIdx,
        ) {
            gtree.assert_inode_invariants(idx);

            if let Either::Internal(inode_idxs) = gtree.inode(idx).children() {
                for &idx in inode_idxs {
                    recursively_assert_inode_invariants(gtree, idx);
                }
            }
        }

        recursively_assert_inode_invariants(self, self.root_idx);

        if let Some(cursor) = self.cursor {
            assert_eq!(self.offset_of_leaf(cursor.leaf_idx), cursor.offset);

            assert_eq!(
                self.idx_of_leaf_in_parent(cursor.leaf_idx),
                cursor.child_idx
            );
        }
    }

    /// Returns the average number of children per internal node.
    ///
    /// Just like a Btree, every internal node in the Gtree contains from a
    /// minimum of `ARITY / 2` to a maximum of `ARITY` children, so the
    /// returned value is always between those two numbers.
    pub fn average_inode_occupancy(&self) -> f32 {
        let total = self.inodes.iter().map(Inode::num_children).sum::<usize>();
        (total as f32) / (self.inodes.len() as f32)
    }

    /// Returns an `(empty_leaves, total_leaves)` tuple.
    ///
    /// Only used for debugging.
    pub fn count_empty_leaves(&self) -> (usize, usize) {
        let empty_leaves = self
            .lnodes
            .iter()
            .map(Lnode::value)
            .filter(|leaf| leaf.is_empty())
            .count();

        (empty_leaves, self.lnodes.len())
    }

    /// Returns a struct whose `Debug` implementation makes it easy to see the
    /// structure of the Gtree by printing it as the equivalent Btree.
    ///
    /// Note, however, that that is *not* how the Gtree is actually stored in
    /// memory. See the `debug_as_self()` method for that.
    pub fn debug_as_btree(&self) -> debug::DebugAsBtree<'_, ARITY, L> {
        self.debug_inode_as_btree(self.root_idx)
    }

    /// Returns a struct whose `Debug` implementation shows how the Gtree is
    /// stored in memory.
    ///
    /// This however makes it really hard to see the hierarchical structure of
    /// the Gtree, from the root node down to the leaf nodes. See the
    /// `debug_as_btree()` method for that.
    pub fn debug_as_self(&self) -> debug::DebugAsSelf<'_, ARITY, L> {
        debug::DebugAsSelf(self)
    }

    /// Deletes all the leaves within the given range.
    ///
    /// # Range contained within a single leaf node
    ///
    /// If the whole range is contained within a single leaf node the
    /// `delete_range` closure is called with the leaf node and the range,
    /// where the start and end are relative to the leaf node and will, in
    /// general, be less than those of the range given to this method (although
    /// the *length* of the range will be the same).
    ///
    /// The closure can then return either a `(None, None)` if the entire leaf
    /// node was deleted, a `(Some, None)` if the range splits the leaf in 2
    /// runs, or a `(Some, Some)` if the range splits the leaf in 3 runs, i.e.
    /// if the start is greater than 0 and the end is less than the leaf's
    /// length.
    ///
    /// Returning a `(None, Some)` is not permitted and will cause a panic.
    ///
    /// The leaves returned by the closure (if any) will be inserted in the
    /// Gtree *after* the leaf node passed to the closure, and this method will
    /// return the indices of those two new leaves.
    ///
    /// In the case the return value of this method is a tuple of the indices
    /// of the leaves returned by the closure, in that order.
    ///
    /// The only exception to this can happen when the closure returns a
    /// `(Some(new_leaf), None)`:
    ///
    /// - if `new_leaf` is empty we'll try to join it with the leaf that comes
    /// *after* the argument of the closure by calling `Join::prepend()`
    /// on it. If that succeeds `new_leaf` gets joined to that leaf so
    /// no new leaves are added to the Gtree, and this methods returns a
    /// `(None, None)`,
    ///
    /// - viceversa, if `new_leaf` is not empty we'll try to join the leaf that
    /// was passed to the closure with the leaf that comes *before* it by
    /// calling `Join::append()` on it. Like in the previous case, if that
    /// succeeds this method returns a `(None, None)`.
    ///
    /// # Range that spans multiple leaf nodes
    ///
    /// This case is a lot more straightforward: the `delete_from` closure is
    /// called with the leaf node that contains the start of the range, and the
    /// `delete_up_to` closure is called with the leaf node that contains the
    /// end of the range. Like in the previous case the length offsets passed
    /// to those closure are relative to the leaf nodes.
    ///
    /// In this case we don't try to do any joining of the leaf nodes, so we
    /// always return the leaf index of the leaves returned by `delete_from`
    /// and `delete_up_to`, in that order.
    #[inline]
    pub fn delete<BeforeDelete, DelRange, DelFrom, DelUpTo>(
        &mut self,
        range: Range<Length>,
        mut before_delete: BeforeDelete,
        delete_range: DelRange,
        delete_from: DelFrom,
        delete_up_to: DelUpTo,
    ) -> (Option<LeafIdx<L>>, Option<LeafIdx<L>>)
    where
        L: Delete,
        BeforeDelete: FnMut(&L),
        DelRange: FnOnce(&mut L, Range<Length>) -> (Option<L>, Option<L>),
        DelFrom: FnOnce(&mut L, Length) -> Option<L>,
        DelUpTo: FnOnce(&mut L, Length) -> Option<L>,
    {
        if let Some(cursor) = self.cursor {
            let cursor_end = cursor.offset + self.leaf(cursor.leaf_idx).len();

            if (cursor.offset..cursor_end).contains_range(range) {
                return self.delete_range_in_leaf(
                    cursor.leaf_idx,
                    cursor.offset,
                    cursor.child_idx,
                    range - cursor.offset,
                    &mut before_delete,
                    delete_range,
                );
            }

            if let Some((leaf_idx, child_idx)) =
                self.next_non_empty_sibling(cursor.leaf_idx, cursor.child_idx)
            {
                let next_len = self.leaf(leaf_idx).len();

                if (cursor_end..cursor_end + next_len).contains_range(range) {
                    return self.delete_range_in_leaf(
                        leaf_idx,
                        cursor_end,
                        child_idx,
                        range - cursor_end,
                        &mut before_delete,
                        delete_range,
                    );
                }
            }
        }

        self.delete_range(
            range,
            &mut before_delete,
            delete_range,
            delete_from,
            delete_up_to,
        )
    }

    #[inline]
    pub fn delete_leaf_range<F, BeforeDelete>(
        &mut self,
        leaf_idx: LeafIdx<L>,
        leaf_offset: Length,
        range: Range<Length>,
        mut before_delete: BeforeDelete,
        delete_with: F,
    ) -> (Option<LeafIdx<L>>, Option<LeafIdx<L>>)
    where
        L: Delete,
        BeforeDelete: FnMut(&L),
        F: FnOnce(&mut L, Range<Length>) -> (Option<L>, Option<L>),
    {
        let idx_in_parent = self.idx_of_leaf_in_parent(leaf_idx);

        self.delete_range_in_leaf(
            leaf_idx,
            leaf_offset,
            idx_in_parent,
            range,
            &mut before_delete,
            delete_with,
        )
    }

    /// Creates a new Gtree with the given leaf as its first leaf.
    #[inline]
    pub fn from_first_leaf(first_leaf: L) -> (Self, LeafIdx<L>) {
        let mut this = Self::uninit();
        let idx = this.initialize(first_leaf);
        (this, idx)
    }

    /// Creates a new Gtree with from an iterator over some leaves and the
    /// total length of the all the leaves the iterator will yield.
    ///
    /// Panics if the iterator yields more than `ARITY` leaves.
    #[inline]
    pub fn from_leaves<I>(leaves: I, tot_len: Length) -> Self
    where
        I: ExactSizeIterator<Item = L>,
    {
        let len = leaves.len();

        let root_idx = InodeIdx(0);

        let mut inode_children = [NodeIdx::dangling(); ARITY];

        let mut lnodes = Vec::with_capacity(leaves.len());

        for (i, child) in leaves.enumerate() {
            let leaf_idx = LeafIdx::new(i);
            inode_children[i] = NodeIdx::from_leaf(leaf_idx);
            lnodes.push(Lnode::new(child, root_idx));
        }

        let inode = Inode {
            tot_len,
            parent: InodeIdx::dangling(),
            num_children: len,
            children: inode_children,
            has_leaves: true,
        };

        let inodes = vec![inode];

        Self { inodes, lnodes, root_idx: InodeIdx(0), cursor: None }
    }

    /// Returns a shared reference to the leaf node at the given index.
    #[inline(always)]
    pub fn leaf(&self, leaf_idx: LeafIdx<L>) -> &L {
        self.lnode(leaf_idx).value()
    }

    #[cfg(feature = "encode")]
    #[inline(always)]
    pub fn num_leaves(&self) -> usize {
        self.lnodes.len()
    }

    #[cfg(feature = "encode")]
    #[inline(always)]
    pub fn inodes(&self) -> &[Inode<ARITY, L>] {
        &self.inodes
    }

    /// Inserts a leaf at the given offset. The offset must be strictly
    /// positive, if it's zero consider using `prepend()` instead.
    ///
    /// The closure is called with the leaf and the corresponding offset
    /// relative to the start of the leaf, which is guaranteed to be strictly
    /// positive.
    ///
    /// It the offset falls exactly between two leaves then the closure will be
    /// called with the leaf preceding the offset.
    ///
    /// The closure can return either a `(Some, Some)`, a `(Some, None)`
    /// or a `(None, None)`. Returning a `(None, Some)` is not permitted and
    /// will cause a panic.
    ///
    /// The leaves returned by the closure (if any) will be inserted in the
    /// Gtree *after* the leaf node passed to the closure, and this method will
    /// return their indices in the Gtree.
    #[inline]
    pub fn insert<F>(
        &mut self,
        offset: Length,
        insert_with: F,
    ) -> (Option<LeafIdx<L>>, Option<LeafIdx<L>>)
    where
        F: FnOnce(&mut L, Length) -> (Option<L>, Option<L>),
    {
        if let Some(cursor) = self.cursor {
            let cursor_end = cursor.offset + self.leaf(cursor.leaf_idx).len();

            if offset > cursor.offset && offset <= cursor_end {
                return self.insert_at_leaf(
                    cursor.leaf_idx,
                    cursor.offset,
                    cursor.child_idx,
                    offset - cursor.offset,
                    insert_with,
                );
            }
        }

        self.insert_at_offset(offset, insert_with)
    }

    /// Inserts a new leaf right after the leaf at the given index.
    ///
    /// Returns the offset of the inserted leaf relative to the start of the
    /// Gtree, and the index of the inserted leaf.
    #[inline]
    pub fn insert_leaf_after_another(
        &mut self,
        leaf: L,
        after_leaf: LeafIdx<L>,
    ) -> (Length, LeafIdx<L>) {
        let (leaf_offset, idx_in_parent) = self
            .cursor
            .and_then(|cursor| {
                if after_leaf == cursor.leaf_idx {
                    Some((cursor.offset, cursor.child_idx))
                } else {
                    None
                }
            })
            .unwrap_or_else(|| {
                let offset = self.offset_of_leaf(after_leaf);
                let idx_in_parent = self.idx_of_leaf_in_parent(after_leaf);
                (offset, idx_in_parent)
            });

        let new_cursor_offset = leaf_offset + self.leaf(after_leaf).len();

        let leaf_idx =
            self.insert_leaf_after_leaf(after_leaf, idx_in_parent, leaf);

        let idx_in_parent = self.idx_of_leaf_in_parent(leaf_idx);

        self.cursor =
            Some(Cursor::new(leaf_idx, new_cursor_offset, idx_in_parent));

        (new_cursor_offset, leaf_idx)
    }

    /// Returns the index of the leaf at the given offset and the offset of
    /// that leaf from the start of the Gtree.
    #[inline]
    pub fn leaf_at_offset(&self, offset: Length) -> (LeafIdx<L>, Length) {
        if let Some(cursor) = self.cursor {
            let cursor_end = cursor.offset + self.leaf(cursor.leaf_idx).len();

            if offset > cursor.offset && offset <= cursor_end {
                return (cursor.leaf_idx, cursor.offset);
            }
        }

        let mut leaf_offset = 0;

        let mut idx = self.root_idx;

        loop {
            let (child_idx, child_offset) =
                self.child_at_offset(idx, offset - leaf_offset);

            leaf_offset += child_offset;

            match self.inode(idx).child(child_idx) {
                Either::Internal(inode_idx) => {
                    idx = inode_idx;
                },
                Either::Leaf(leaf_idx) => {
                    return (leaf_idx, leaf_offset);
                },
            }
        }
    }

    /// Returns an iterator over the leaves of the Gtree starting from the
    /// given leaf.
    #[inline]
    pub fn leaves<const INCLUDE_START: bool>(
        &self,
        start_leaf: LeafIdx<L>,
    ) -> Leaves<'_, ARITY, L> {
        Leaves::new::<INCLUDE_START>(self, start_leaf)
    }

    /// Returns an iterator over the leaves of the Gtree starting from the
    /// first leaf.
    #[inline]
    pub fn leaves_from_first(&self) -> Leaves<'_, ARITY, L> {
        let first_leaf = self.first_leaf_idx();
        Leaves::new::<true>(self, first_leaf)
    }

    /// Returns the combined length of all the leaves in the Gtree.
    #[inline(always)]
    pub fn len(&self) -> Length {
        self.root().len()
    }

    /// Creates a new Gtree with the given leaf as its first leaf.
    #[inline(always)]
    pub fn new(
        inodes: Vec<Inode<ARITY, L>>,
        lnodes: Vec<Lnode<L>>,
        root_idx: InodeIdx,
    ) -> Self {
        Self { inodes, lnodes, root_idx, cursor: None }
    }

    /// Returns the index of the leaf that's directly after the leaf at the
    /// given index.
    #[inline]
    pub fn next_leaf(&self, leaf_idx: LeafIdx<L>) -> Option<LeafIdx<L>> {
        let idx_in_parent = self.idx_of_leaf_in_parent(leaf_idx);

        let parent_idx = self.lnode(leaf_idx).parent();

        let parent = self.inode(parent_idx);

        if idx_in_parent + 1 < parent.num_children() {
            return Some(parent.child(idx_in_parent + 1).unwrap_leaf());
        }

        let mut inode_idx = parent_idx;

        loop {
            if self.is_root(inode_idx) {
                return None;
            }

            let idx_in_parent = self.idx_of_inode_in_parent(inode_idx);
            let parent_idx = self.inode(inode_idx).parent();
            let parent = self.inode(parent_idx);

            if idx_in_parent + 1 < parent.num_children() {
                inode_idx = parent.child(idx_in_parent + 1).unwrap_internal();
                break;
            } else {
                inode_idx = parent_idx;
            }
        }

        loop {
            match self.inode(inode_idx).children() {
                Either::Internal(inode_idxs) => {
                    inode_idx = inode_idxs[0];
                },

                Either::Leaf(leaf_idxs) => {
                    return Some(leaf_idxs[0]);
                },
            }
        }
    }

    /// Returns the offset of the given leaf from the start of the Gtree.
    #[inline]
    pub fn offset_of_leaf(&self, leaf_idx: LeafIdx<L>) -> Length {
        if let Some(cursor) = self.cursor {
            if leaf_idx == cursor.leaf_idx {
                return cursor.offset;
            }
        }

        let mut offset = 0;

        offset += self.offset_of_leaf_child(leaf_idx);

        let mut inode_idx = self.lnode(leaf_idx).parent();

        while !self.is_root(inode_idx) {
            offset += self.offset_of_internal_child(inode_idx);
            inode_idx = self.inode(inode_idx).parent();
        }

        offset
    }

    /// Returns the [`InodeIdx`] of the parent node of the given leaf.
    #[cfg(feature = "encode")]
    #[inline(always)]
    pub fn parent(&self, leaf_idx: LeafIdx<L>) -> InodeIdx {
        self.lnode(leaf_idx).parent()
    }

    /// Prepends a new leaf node to start of the Gtree, returning its newly
    /// created leaf index.
    #[inline]
    pub fn prepend(&mut self, leaf: L) -> LeafIdx<L> {
        let first_leaf_idx = self.first_leaf_idx();
        let leaf_idx = self.insert_leaf_before_leaf(first_leaf_idx, 0, leaf);
        self.cursor = Some(Cursor::new(leaf_idx, 0, 0));
        leaf_idx
    }

    /// Returns the index of the leaf that's directly before the leaf at the
    /// given index. It panics if the given index is that of the first leaf.
    #[inline]
    pub fn prev_leaf(&self, leaf_idx: LeafIdx<L>) -> Option<LeafIdx<L>> {
        let idx_in_parent = self.idx_of_leaf_in_parent(leaf_idx);

        let parent_idx = self.lnode(leaf_idx).parent();

        let parent = self.inode(parent_idx);

        if idx_in_parent > 0 {
            return Some(parent.child(idx_in_parent - 1).unwrap_leaf());
        }

        let mut inode_idx = parent_idx;

        loop {
            if self.is_root(inode_idx) {
                return None;
            }

            let idx_in_parent = self.idx_of_inode_in_parent(inode_idx);
            let parent_idx = self.inode(inode_idx).parent();
            let parent = self.inode(parent_idx);

            if idx_in_parent > 0 {
                inode_idx = parent.child(idx_in_parent - 1).unwrap_internal();
                break;
            } else {
                inode_idx = parent_idx;
            }
        }

        loop {
            match self.inode(inode_idx).children() {
                Either::Internal(inode_idxs) => {
                    inode_idx = inode_idxs[inode_idxs.len() - 1];
                },

                Either::Leaf(leaf_idxs) => {
                    return Some(leaf_idxs[leaf_idxs.len() - 1]);
                },
            }
        }
    }

    /// Removes the cursor.
    #[inline]
    pub fn remove_cursor(&mut self) {
        self.cursor = None;
    }

    /// Returns an iterator over the siblings of the leaf at the given index.
    #[inline]
    pub fn siblings<const INCLUDE_LEAF: bool>(
        &self,
        leaf_idx: LeafIdx<L>,
    ) -> Siblings<'_, ARITY, L> {
        Siblings::new::<INCLUDE_LEAF>(self, leaf_idx)
    }

    /// Calls the closure with the leaf at the given index. The closure should
    /// split the into two leaves, returning the right part of the split.
    ///
    /// Panics if the leaf's length before calling this function is not
    /// equal to the sum of the new length and the length of the returned leaf.
    #[inline]
    pub fn split_leaf<F>(
        &mut self,
        leaf_idx: LeafIdx<L>,
        split_with: F,
    ) -> LeafIdx<L>
    where
        F: FnOnce(&mut L) -> L,
    {
        let lnode = self.lnode_mut(leaf_idx);
        let parent_idx = lnode.parent();

        let old_len = lnode.len();
        let split_leaf = split_with(lnode.value_mut());
        let new_len = lnode.len();

        debug_assert!(new_len + split_leaf.len() == old_len);

        if old_len != new_len {
            let diff = Length::diff(old_len, new_len);
            self.patch(parent_idx, diff);
        }

        let idx_in_parent = self.inode(parent_idx).idx_of_leaf_child(leaf_idx);

        self.insert_leaf_after_leaf(leaf_idx, idx_in_parent, split_leaf)
    }

    /// Calls the closure with the leaf at the given index. The closure should
    /// insert a new leaf by splitting the given leaf into two, returning the
    /// new leaf and the right part of the split.
    ///
    /// Returns the offset of the inserted leaf relative to the start of the
    /// Gtree, its index and the index of the right part of the split.
    #[inline]
    pub fn split_leaf_with_another<F>(
        &mut self,
        leaf_idx: LeafIdx<L>,
        split_with: F,
    ) -> (Length, LeafIdx<L>, LeafIdx<L>)
    where
        F: FnOnce(&mut L) -> (L, L),
    {
        let (leaf_offset, idx_in_parent) = self
            .cursor
            .and_then(|cursor| {
                if leaf_idx == cursor.leaf_idx {
                    Some((cursor.offset, cursor.child_idx))
                } else {
                    None
                }
            })
            .unwrap_or_else(|| {
                let offset = self.offset_of_leaf(leaf_idx);
                let idx_in_parent = self.idx_of_leaf_in_parent(leaf_idx);
                (offset, idx_in_parent)
            });

        let lnode = self.lnode_mut(leaf_idx);
        let parent_idx = lnode.parent();

        let old_len = lnode.len();
        let (inserted_leaf, split_leaf) = split_with(lnode.value_mut());
        let new_len = lnode.len();

        let new_cursor_offset = leaf_offset + new_len;

        debug_assert!(new_len + split_leaf.len() == old_len);

        let diff = Length::diff(old_len, new_len);

        self.patch(parent_idx, diff);

        let (inserted_idx, split_idx) = self.insert_two_leaves_after_leaf(
            leaf_idx,
            idx_in_parent,
            inserted_leaf,
            split_leaf,
        );

        let idx_in_parent = self.idx_of_leaf_in_parent(inserted_idx);

        self.cursor =
            Some(Cursor::new(inserted_idx, new_cursor_offset, idx_in_parent));

        (new_cursor_offset, inserted_idx, split_idx)
    }

    /// Calls the closure with a mutable reference to the last leaf of the
    /// Gtree.
    #[inline]
    pub fn with_last_leaf_mut<F>(&mut self, with_leaf: F)
    where
        F: FnOnce(&mut L),
    {
        let (last_idx, _) = self.last_leaf();
        self.with_leaf_mut(last_idx, with_leaf);
    }

    /// Calls the closure with a mutable reference to the leaf at the given
    /// index.
    #[inline]
    pub fn with_leaf_mut<F>(&mut self, leaf_idx: LeafIdx<L>, with_leaf: F)
    where
        F: FnOnce(&mut L),
    {
        let lnode = self.lnode_mut(leaf_idx);

        let old_len = lnode.len();
        with_leaf(lnode.value_mut());
        let new_len = lnode.len();

        if old_len != new_len {
            let diff = Length::diff(old_len, new_len);
            let parent_idx = lnode.parent();
            self.patch(parent_idx, diff);
        }
    }

    /// Calls the closure with exclusive references to the two leaves the given
    /// indices. The order of the references passed to the closure matches the
    /// order of the indices given as arguments.
    #[inline]
    pub fn with_two_mut<F>(
        &mut self,
        first_idx: LeafIdx<L>,
        second_idx: LeafIdx<L>,
        with_two: F,
    ) where
        F: FnOnce(&mut L, &mut L),
    {
        debug_assert!(first_idx != second_idx);

        let (first, second) = crate::get_two_mut(
            &mut self.lnodes,
            first_idx.idx,
            second_idx.idx,
        );

        let first_parent = first.parent();

        let old_first_len = first.value().len();

        let second_parent = second.parent();

        let old_second_len = second.value().len();

        with_two(first.value_mut(), second.value_mut());

        let first_diff = Length::diff(old_first_len, first.value().len());

        let second_diff = Length::diff(old_second_len, second.value().len());

        self.patch(first_parent, first_diff);

        self.patch(second_parent, second_diff);
    }
}

// Private API.
impl<const ARITY: usize, L: Leaf> Gtree<ARITY, L> {
    /// Applies the diff to the len of every internal node starting
    /// from the given inode and going up the tree until the root.
    #[inline]
    fn patch(&mut self, starting_from: InodeIdx, diff: Diff) {
        let mut idx = starting_from;

        while !idx.is_dangling() {
            let inode = self.inode_mut(idx);
            inode.len_mut().patch(diff);
            idx = inode.parent();
        }
    }

    /// Asserts the invariants of the given inode. Does not recurse into its
    /// children.
    fn assert_inode_invariants(&self, inode_idx: InodeIdx) {
        let inode = self.inode(inode_idx);

        let inode_height = self.inode_height(inode_idx);

        let mut child_lengths = 0;

        match inode.children() {
            Either::Internal(inode_idxs) => {
                for &child_idx in inode_idxs {
                    let child = self.inode(child_idx);
                    assert_eq!(child.parent(), inode_idx);
                    assert_eq!(self.inode_height(child_idx) + 1, inode_height);
                    child_lengths += child.len();
                }
            },

            Either::Leaf(leaf_idxs) => {
                assert_eq!(inode_height, 1);

                for &child_idx in leaf_idxs {
                    let child = self.lnode(child_idx);
                    assert_eq!(child.parent(), inode_idx);
                    child_lengths += child.value().len();
                }
            },
        }

        if !inode.len() == 0 {
            assert_eq!(child_lengths, inode.len());
        }
    }

    /// Inserts `maybe_split` after `inode_idx` in the parent of `inode_idx`.
    ///
    /// It that causes another split, it recurses up the tree.
    ///
    /// `diff` is the diff of `inode_idx`'s parent before and after inserting
    /// the node(s) that caused `maybe_split` to be created.
    ///
    /// `leaf_diff` is the diff that will be bubbled up the tree as soon as
    /// `maybe_split` becomes `None`.
    #[inline]
    fn bubble(
        &mut self,
        mut inode_idx: InodeIdx,
        mut maybe_split: Option<Inode<ARITY, L>>,
        mut diff: Diff,
        leaf_diff: Diff,
    ) {
        let mut parent_idx = self.inode(inode_idx).parent();

        loop {
            if parent_idx.is_dangling() {
                if let Some(split) = maybe_split {
                    self.root_has_split(split);
                }
                return;
            } else if let Some(split) = maybe_split {
                let split_len = split.len();
                let split_idx = self.push_inode(split, parent_idx);

                let parent = self.inode_mut(parent_idx);
                let old_len = parent.len();
                parent.len_mut().patch(diff);
                let idx_in_parent = parent.idx_of_internal_child(inode_idx);

                maybe_split = self.insert_in_inode(
                    parent_idx,
                    idx_in_parent + 1,
                    NodeIdx::from_internal(split_idx),
                    split_len,
                );

                let parent = self.inode(parent_idx);
                diff = Length::diff(old_len, parent.len());
                inode_idx = parent_idx;
                parent_idx = parent.parent();
            } else {
                self.patch(parent_idx, leaf_diff);
                return;
            }
        }
    }

    /// Returns the index and len offsets of the inode's child at the given
    /// length offset.
    ///
    /// If the length offset falls between two children, then the infos of the
    /// child that follows the length offset are returned.
    #[inline]
    fn child_at_offset(
        &self,
        of_inode: InodeIdx,
        at_offset: Length,
    ) -> (ChildIdx, Length) {
        debug_assert!(at_offset <= self.inode(of_inode).len());

        let mut offset = 0;

        match self.inode(of_inode).children() {
            Either::Internal(inode_idxs) => {
                for (idx, &inode_idx) in inode_idxs.iter().enumerate() {
                    let len = self.inode(inode_idx).len();
                    offset += len;
                    if offset >= at_offset {
                        return (idx, offset - len);
                    }
                }
            },

            Either::Leaf(leaf_idxs) => {
                for (idx, &leaf_idx) in leaf_idxs.iter().enumerate() {
                    let len = self.leaf(leaf_idx).len();
                    offset += len;
                    if offset >= at_offset {
                        return (idx, offset - len);
                    }
                }
            },
        }

        unreachable!();
    }

    /// Returns the index and length offsets of the inode's child that fully
    /// contains the given length range, or `None` if no such child exists.
    #[inline]
    fn child_containing_range(
        &self,
        of_inode: InodeIdx,
        range: Range<Length>,
    ) -> Option<(ChildIdx, Length)> {
        debug_assert!(range.start <= range.end);
        debug_assert!(range.end <= self.inode(of_inode).len());

        #[inline(always)]
        fn measure<L, I>(
            iter: I,
            range: Range<Length>,
        ) -> Option<(ChildIdx, Length)>
        where
            L: Leaf,
            I: Iterator<Item = Length>,
        {
            let mut offset = 0;
            for (idx, child_len) in iter.enumerate() {
                offset += child_len;
                if offset > range.start {
                    return if offset >= range.end {
                        Some((idx, offset - child_len))
                    } else {
                        None
                    };
                }
            }
            unreachable!();
        }

        match self.inode(of_inode).children() {
            Either::Internal(inode_idxs) => {
                let iter = inode_idxs.iter().map(|&idx| self.inode(idx).len());
                measure::<L, _>(iter, range)
            },

            Either::Leaf(leaf_idxs) => {
                let iter = leaf_idxs.iter().map(|&idx| self.leaf(idx).len());
                measure::<L, _>(iter, range)
            },
        }
    }

    /// Returns the length of the inode's child at the given index.
    #[inline]
    fn child_measure(
        &self,
        of_inode: InodeIdx,
        child_idx: ChildIdx,
    ) -> Length {
        match self.inode(of_inode).child(child_idx) {
            Either::Internal(inode_idx) => self.inode(inode_idx).len(),
            Either::Leaf(leaf_idx) => self.leaf(leaf_idx).len(),
        }
    }

    fn debug_inode_as_btree(
        &self,
        inode_idx: InodeIdx,
    ) -> debug::DebugAsBtree<'_, ARITY, L> {
        debug::DebugAsBtree { gtree: self, inode_idx }
    }

    #[inline]
    fn delete_child<BeforeDelete>(
        &mut self,
        of_inode: InodeIdx,
        child_idx: ChildIdx,
        before_delete: &mut BeforeDelete,
    ) where
        L: Delete,
        BeforeDelete: FnMut(&L),
    {
        let child_len = match self.inode(of_inode).child(child_idx) {
            Either::Internal(inode_idx) => {
                let inode = self.inode_mut(inode_idx);
                let old_len = inode.len();
                self.delete_inode(inode_idx, before_delete);
                old_len
            },
            Either::Leaf(leaf_idx) => {
                let leaf = self.leaf_mut(leaf_idx);
                before_delete(leaf);
                let old_len = leaf.len();
                leaf.delete();
                old_len
            },
        };

        *self.inode_mut(of_inode).len_mut() -= child_len;
    }

    /// Recursively deletes the given inode and all of its children.
    #[inline]
    fn delete_inode<BeforeDelete>(
        &mut self,
        inode: InodeIdx,
        before_delete: &mut BeforeDelete,
    ) where
        L: Delete,
        BeforeDelete: FnMut(&L),
    {
        let inode = self.inode_mut(inode);

        *inode.len_mut() = 0;

        // In both arms of the match we transmute the slices back into the same
        // types to iterate over the inode/leaf indices and delete the
        // inodes/leaves at the same time, which Rust's aliasing rules would
        // prohibit.
        //
        // SAFETY: it's safe.

        match inode.children() {
            Either::Internal(inode_idxs) => {
                let inode_idxs =
                    unsafe { mem::transmute::<_, &[InodeIdx]>(inode_idxs) };
                for &inode_idx in inode_idxs {
                    self.delete_inode(inode_idx, before_delete);
                }
            },

            Either::Leaf(leaf_idxs) => {
                let leaf_idxs =
                    unsafe { mem::transmute::<_, &[LeafIdx<L>]>(leaf_idxs) };
                for &leaf_idx in leaf_idxs {
                    let leaf = self.leaf_mut(leaf_idx);
                    before_delete(leaf);
                    leaf.delete();
                }
            },
        }
    }

    #[inline]
    fn delete_range_in_leaf<BeforeDelete, F>(
        &mut self,
        leaf_idx: LeafIdx<L>,
        leaf_offset: Length,
        idx_in_parent: ChildIdx,
        range: Range<Length>,
        before_delete: &mut BeforeDelete,
        delete_with: F,
    ) -> (Option<LeafIdx<L>>, Option<LeafIdx<L>>)
    where
        L: Delete,
        BeforeDelete: FnMut(&L),
        F: FnOnce(&mut L, Range<Length>) -> (Option<L>, Option<L>),
    {
        debug_assert!(range.start < range.end);
        debug_assert!(range.len() <= self.leaf(leaf_idx).len());
        debug_assert_eq!(idx_in_parent, self.idx_of_leaf_in_parent(leaf_idx));
        debug_assert_eq!(leaf_offset, self.offset_of_leaf(leaf_idx));

        let lnode = self.lnode_mut(leaf_idx);

        before_delete(lnode.value());

        let old_len = lnode.len();

        let (first, second) = delete_with(lnode.value_mut(), range);

        let diff = Length::diff(old_len, lnode.len());

        let parent_idx = lnode.parent();

        self.patch(parent_idx, diff);

        match (first, second) {
            (Some(first), Some(second)) => {
                let (first_idx, second_idx) = self
                    .insert_two_leaves_after_leaf(
                        leaf_idx,
                        idx_in_parent,
                        first,
                        second,
                    );

                let idx_in_parent = self
                    .update_child_idx_after_inserting_after(
                        parent_idx,
                        idx_in_parent,
                    );

                self.cursor =
                    Some(Cursor::new(leaf_idx, leaf_offset, idx_in_parent));

                (Some(first_idx), Some(second_idx))
            },

            (Some(deleted), None) if deleted.is_empty() => {
                let inserted_idx =
                    match self.next_sibling(leaf_idx, idx_in_parent) {
                        Some(next_idx) => {
                            self.leaf_mut(next_idx).prepend(deleted).err().map(
                                |deleted| {
                                    self.insert_leaf_after_leaf(
                                        leaf_idx,
                                        idx_in_parent,
                                        deleted,
                                    )
                                },
                            )
                        },

                        _ => Some(self.insert_leaf_after_leaf(
                            leaf_idx,
                            idx_in_parent,
                            deleted,
                        )),
                    };

                let idx_in_parent = self
                    .update_child_idx_after_inserting_after(
                        parent_idx,
                        idx_in_parent,
                    );

                self.cursor =
                    Some(Cursor::new(leaf_idx, leaf_offset, idx_in_parent));

                (inserted_idx, None)
            },

            (Some(rest), None) => {
                debug_assert!(self.leaf(leaf_idx).is_empty());

                let inserted_idx = match self
                    .prev_sibling(leaf_idx, idx_in_parent)
                {
                    Some(prev_idx) => {
                        let diff = Length::diff(0, rest.len());

                        let deleted =
                            core::mem::replace(self.leaf_mut(leaf_idx), rest);

                        let previous_leaf = self.leaf_mut(prev_idx);

                        if let Err(deleted) = previous_leaf.append(deleted) {
                            let rest = core::mem::replace(
                                self.leaf_mut(leaf_idx),
                                deleted,
                            );

                            Some(self.insert_leaf_after_leaf(
                                leaf_idx,
                                idx_in_parent,
                                rest,
                            ))
                        } else {
                            self.patch(parent_idx, diff);
                            None
                        }
                    },

                    _ => Some(self.insert_leaf_after_leaf(
                        leaf_idx,
                        idx_in_parent,
                        rest,
                    )),
                };

                let idx_in_parent = self
                    .update_child_idx_after_inserting_after(
                        parent_idx,
                        idx_in_parent,
                    );

                self.cursor = self
                    .prev_non_empty_sibling(leaf_idx, idx_in_parent)
                    .map(|(previous_idx, child_idx)| {
                        let previous_len = self.leaf(previous_idx).len();
                        Cursor::new(
                            previous_idx,
                            leaf_offset - previous_len,
                            child_idx,
                        )
                    });

                (inserted_idx, None)
            },

            (None, None) => {
                debug_assert!(self.leaf(leaf_idx).is_empty());

                self.cursor = self
                    .prev_non_empty_sibling(leaf_idx, idx_in_parent)
                    .map(|(previous_idx, child_idx)| {
                        let previous_len = self.leaf(previous_idx).len();
                        Cursor::new(
                            previous_idx,
                            leaf_offset - previous_len,
                            child_idx,
                        )
                    });

                (None, None)
            },

            _ => unreachable!(),
        }
    }

    #[inline]
    fn delete_range<BeforeDelete, DelRange, DelFrom, DelUpTo>(
        &mut self,
        mut range: Range<Length>,
        before_delete: &mut BeforeDelete,
        delete_range: DelRange,
        delete_from: DelFrom,
        delete_up_to: DelUpTo,
    ) -> (Option<LeafIdx<L>>, Option<LeafIdx<L>>)
    where
        L: Delete,
        BeforeDelete: FnMut(&L),
        DelRange: FnOnce(&mut L, Range<Length>) -> (Option<L>, Option<L>),
        DelFrom: FnOnce(&mut L, Length) -> Option<L>,
        DelUpTo: FnOnce(&mut L, Length) -> Option<L>,
    {
        // First, descend to the deepest node that fully contains the range.

        let mut idx = self.root_idx;

        let mut leaf_offset = 0;

        loop {
            match self.child_containing_range(idx, range) {
                // A child of this inode fully contains the range..
                Some((child_idx, offset)) => {
                    leaf_offset += offset;
                    range.start -= offset;
                    range.end -= offset;

                    match self.inode(idx).child(child_idx) {
                        // ..and it's another inode, so we keep descending.
                        Either::Internal(inode_idx) => {
                            idx = inode_idx;
                        },

                        // ..and it's a leaf, so we're done descending and
                        // we can delete the range in this leaf.
                        Either::Leaf(leaf_idx) => {
                            return self.delete_range_in_leaf(
                                leaf_idx,
                                leaf_offset,
                                child_idx,
                                range,
                                before_delete,
                                delete_range,
                            );
                        },
                    }
                },

                // No child of this inode fully contains the range, so we call
                // `delete_inode_range` which deletes ranges that span multiple
                // children.
                None => {
                    return self.delete_range_in_inode(
                        idx,
                        range,
                        before_delete,
                        delete_from,
                        delete_up_to,
                    );
                },
            }
        }
    }

    #[inline]
    fn delete_range_in_inode<BeforeDelete, DelFrom, DelUpTo>(
        &mut self,
        idx: InodeIdx,
        range: Range<Length>,
        before_delete: &mut BeforeDelete,
        delete_from: DelFrom,
        delete_up_to: DelUpTo,
    ) -> (Option<LeafIdx<L>>, Option<LeafIdx<L>>)
    where
        L: Delete,
        BeforeDelete: FnMut(&L),
        DelFrom: FnOnce(&mut L, Length) -> Option<L>,
        DelUpTo: FnOnce(&mut L, Length) -> Option<L>,
    {
        debug_assert!(self.child_containing_range(idx, range).is_none());

        self.cursor = None;

        let old_len = self.inode(idx).len();

        let ((first, second), maybe_split) = delete::delete_range_in_inode(
            self,
            idx,
            range,
            before_delete,
            delete_from,
            delete_up_to,
        );

        let diff = Length::diff(old_len, self.inode(idx).len());

        let split_len = maybe_split.as_ref().map(Inode::len).unwrap_or(0);

        let leaf_diff =
            Length::diff(old_len, self.inode(idx).len() + split_len);

        self.bubble(idx, maybe_split, diff, leaf_diff);

        (first, second)
    }

    #[inline]
    fn first_leaf_idx(&self) -> LeafIdx<L> {
        let mut idx = self.root_idx;

        loop {
            match self.inode(idx).children() {
                Either::Internal(children) => {
                    idx = children[0];
                },
                Either::Leaf(leaf_idx) => {
                    break leaf_idx[0];
                },
            }
        }
    }

    #[allow(dead_code)]
    #[inline(always)]
    fn height(&self) -> usize {
        self.inode_height(self.root_idx)
    }

    #[inline(always)]
    fn idx_of_inode_in_parent(&self, inode_idx: InodeIdx) -> ChildIdx {
        let parent = self.inode(self.inode(inode_idx).parent());
        parent.idx_of_internal_child(inode_idx)
    }

    #[inline(always)]
    fn idx_of_leaf_in_parent(&self, leaf_idx: LeafIdx<L>) -> ChildIdx {
        let parent = self.inode(self.lnode(leaf_idx).parent());
        parent.idx_of_leaf_child(leaf_idx)
    }

    /// Initializes the Gtree with the first leaf.
    #[inline]
    fn initialize(&mut self, first_leaf: L) -> LeafIdx<L> {
        let len = first_leaf.len();
        let leaf_idx = self.push_leaf(first_leaf, InodeIdx(0));
        let root = Inode::from_leaf(leaf_idx, len, InodeIdx::dangling());
        self.root_idx = self.push_inode(root, InodeIdx::dangling());
        leaf_idx
    }

    #[inline(always)]
    fn inode(&self, idx: InodeIdx) -> &Inode<ARITY, L> {
        &self.inodes[idx.0]
    }

    /// Returns the height of the subtree rooted at `idx`.
    #[inline(always)]
    fn inode_height(&self, mut idx: InodeIdx) -> usize {
        let mut height = 1;

        while let Either::Internal(idxs) = self.inode(idx).children() {
            idx = *idxs.first().unwrap();
            height += 1;
        }

        height
    }

    #[inline]
    fn inode_mut(&mut self, idx: InodeIdx) -> &mut Inode<ARITY, L> {
        &mut self.inodes[idx.0]
    }

    #[inline]
    fn insert_at_leaf<F>(
        &mut self,
        leaf_idx: LeafIdx<L>,
        leaf_offset: Length,
        idx_in_parent: ChildIdx,
        insert_at_offset: Length,
        insert_with: F,
    ) -> (Option<LeafIdx<L>>, Option<LeafIdx<L>>)
    where
        F: FnOnce(&mut L, Length) -> (Option<L>, Option<L>),
    {
        debug_assert!(insert_at_offset > 0);

        let lnode = self.lnode_mut(leaf_idx);

        let old_len = lnode.len();

        let (first, second) = insert_with(lnode.value_mut(), insert_at_offset);

        let new_len = lnode.len();

        let diff = Length::diff(old_len, new_len);

        let parent_idx = lnode.parent();

        self.patch(parent_idx, diff);

        let (first_idx, second_idx) = match (first, second) {
            (None, None) => (None, None),

            (Some(first), Some(second)) => {
                let (first_idx, second_idx) = self
                    .insert_two_leaves_after_leaf(
                        leaf_idx,
                        idx_in_parent,
                        first,
                        second,
                    );

                (Some(first_idx), Some(second_idx))
            },

            (Some(first), None) => {
                let first_idx = self.insert_leaf_after_leaf(
                    leaf_idx,
                    idx_in_parent,
                    first,
                );

                (Some(first_idx), None)
            },

            _ => unreachable!(),
        };

        self.cursor = if let Some(first_idx) = first_idx {
            let idx_in_parent = self.update_child_idx_after_inserting_after(
                parent_idx,
                idx_in_parent + 1,
            );

            Some(Cursor::new(first_idx, leaf_offset + new_len, idx_in_parent))
        } else {
            let idx_in_parent = self.update_child_idx_after_inserting_after(
                parent_idx,
                idx_in_parent,
            );

            Some(Cursor::new(leaf_idx, leaf_offset, idx_in_parent))
        };

        (first_idx, second_idx)
    }

    #[inline]
    fn insert_at_offset<F>(
        &mut self,
        offset: Length,
        insert_with: F,
    ) -> (Option<LeafIdx<L>>, Option<LeafIdx<L>>)
    where
        F: FnOnce(&mut L, Length) -> (Option<L>, Option<L>),
    {
        let (idxs, maybe_split) = insert::insert_at_offset(
            self,
            self.root_idx,
            0,
            offset,
            insert_with,
        );

        if let Some(root_split) = maybe_split {
            self.root_has_split(root_split);
        }

        idxs
    }

    #[inline]
    fn insert_in_inode(
        &mut self,
        idx: InodeIdx,
        at_offset: usize,
        child: NodeIdx<L>,
        child_len: Length,
    ) -> Option<Inode<ARITY, L>> {
        let min_children = Inode::<ARITY, L>::min_children();

        let inode = self.inode_mut(idx);

        debug_assert!(at_offset <= inode.num_children());

        if inode.is_full() {
            let split_offset = inode.num_children() - min_children;

            // Split so that the extra inode always has the minimum number
            // of children.
            let rest = if at_offset <= min_children {
                let rest = self.split_inode(idx, split_offset);
                self.inode_mut(idx).insert(at_offset, child, child_len);
                rest
            } else {
                let mut rest = self.split_inode(idx, split_offset + 1);
                rest.insert(
                    at_offset - self.inode(idx).num_children(),
                    child,
                    child_len,
                );
                rest
            };

            debug_assert_eq!(rest.num_children(), min_children);

            Some(rest)
        } else {
            inode.insert(at_offset, child, child_len);
            None
        }
    }

    /// Inserts the given leaf after the leaf at `leaf_idx`.
    ///
    /// `idx_in_parent` is the child index of `leaf_idx` in its parent.
    ///
    /// Note that this function does not update the cursor. The caller is
    /// responsible for doing that.
    #[inline]
    fn insert_leaf_after_leaf(
        &mut self,
        leaf_idx: LeafIdx<L>,
        idx_in_parent: ChildIdx,
        leaf: L,
    ) -> LeafIdx<L> {
        let leaf_len = leaf.len();

        let leaf_diff = Length::diff(0, leaf_len);

        let parent_idx = self.lnode(leaf_idx).parent();

        let inserted_idx = self.push_leaf(leaf, parent_idx);

        let parent = self.inode(parent_idx);
        let old_len = parent.len();

        let maybe_split = self.insert_in_inode(
            parent_idx,
            idx_in_parent + 1,
            NodeIdx::from_leaf(inserted_idx),
            leaf_len,
        );

        let parent = self.inode(parent_idx);
        let diff = Length::diff(old_len, parent.len());

        self.bubble(parent_idx, maybe_split, diff, leaf_diff);

        inserted_idx
    }

    /// Inserts the given leaf before the leaf at `leaf_idx`.
    ///
    /// `idx_in_parent` is the child index of `leaf_idx` in its parent.
    ///
    /// Note that this function does not update the cursor. The caller is
    /// responsible for doing that.
    #[inline]
    fn insert_leaf_before_leaf(
        &mut self,
        leaf_idx: LeafIdx<L>,
        idx_in_parent: ChildIdx,
        leaf: L,
    ) -> LeafIdx<L> {
        let leaf_len = leaf.len();

        let leaf_iff = Length::diff(0, leaf_len);

        let parent_idx = self.lnode(leaf_idx).parent();

        let inserted_idx = self.push_leaf(leaf, parent_idx);

        let parent = self.inode(parent_idx);
        let old_len = parent.len();

        let maybe_split = self.insert_in_inode(
            parent_idx,
            idx_in_parent,
            NodeIdx::from_leaf(inserted_idx),
            leaf_len,
        );

        let parent = self.inode(parent_idx);
        let diff = Length::diff(old_len, parent.len());

        self.bubble(parent_idx, maybe_split, diff, leaf_iff);

        inserted_idx
    }

    /// Inserts two nodes into the given inode at the given offsets. It returns
    /// a new inode at the same depth of the inode at `idx` if the insertion
    /// caused the inode to split.
    #[allow(clippy::too_many_arguments)]
    #[inline]
    fn insert_two_in_inode(
        &mut self,
        idx: InodeIdx,
        mut first_offset: usize,
        mut first_child: NodeIdx<L>,
        mut first_len: Length,
        mut second_offset: usize,
        mut second_child: NodeIdx<L>,
        mut second_len: Length,
    ) -> Option<Inode<ARITY, L>> {
        use core::cmp::Ordering;

        let min_children = Inode::<ARITY, L>::min_children();

        debug_assert!(min_children >= 2);

        if first_offset > second_offset {
            (
                first_child,
                second_child,
                first_offset,
                second_offset,
                first_len,
                second_len,
            ) = (
                second_child,
                first_child,
                second_offset,
                first_offset,
                second_len,
                first_len,
            )
        }

        let max_children = Inode::<ARITY, L>::max_children();

        let inode = self.inode_mut(idx);

        let len = inode.num_children();

        debug_assert!(second_offset <= inode.num_children());

        if max_children - len < 2 {
            let split_offset = len - min_children;

            let children_after_second = len - second_offset;

            // Split so that the extra inode always has the minimum number
            // of children.
            //
            // The logic to make this work is a bit annoying to reason
            // about. We should probably add some unit tests to avoid
            // possible regressions.
            let rest = match children_after_second.cmp(&(min_children - 1)) {
                Ordering::Greater => {
                    let rest = self.split_inode(idx, split_offset);
                    self.insert_two_in_inode(
                        idx,
                        first_offset,
                        first_child,
                        first_len,
                        second_offset,
                        second_child,
                        second_len,
                    );
                    rest
                },

                Ordering::Less if first_offset >= split_offset + 2 => {
                    let mut rest = self.split_inode(idx, split_offset + 2);
                    let new_len = self.inode(idx).num_children();
                    first_offset -= new_len;
                    second_offset -= new_len;
                    rest.insert_two(
                        first_offset,
                        first_child,
                        first_len,
                        second_offset,
                        second_child,
                        second_len,
                    );
                    rest
                },

                _ => {
                    let mut rest = self.split_inode(idx, split_offset + 1);
                    let new_len = self.inode(idx).num_children();
                    rest.insert(
                        second_offset - new_len,
                        second_child,
                        second_len,
                    );
                    self.inode_mut(idx).insert(
                        first_offset,
                        first_child,
                        first_len,
                    );
                    rest
                },
            };

            debug_assert_eq!(rest.num_children(), min_children);

            Some(rest)
        } else {
            inode.insert_two(
                first_offset,
                first_child,
                first_len,
                second_offset,
                second_child,
                second_len,
            );
            None
        }
    }

    /// Same as [`insert_leaf_after_leaf()`] but inserts two leaves instead of
    /// one.
    ///
    /// Note that this function does not update the cursor. The caller is
    /// responsible for doing that.
    #[inline]
    fn insert_two_leaves_after_leaf(
        &mut self,
        leaf_idx: LeafIdx<L>,
        idx_in_parent: ChildIdx,
        first_leaf: L,
        second_leaf: L,
    ) -> (LeafIdx<L>, LeafIdx<L>) {
        let first_len = first_leaf.len();
        let second_len = second_leaf.len();

        let leaf_diff = Length::diff(0, first_len + second_len);

        let parent_idx = self.lnode(leaf_idx).parent();

        let first_idx = self.push_leaf(first_leaf, parent_idx);
        let second_idx = self.push_leaf(second_leaf, parent_idx);

        let parent = self.inode(parent_idx);
        let old_len = parent.len();

        let maybe_split = self.insert_two_in_inode(
            parent_idx,
            idx_in_parent + 1,
            NodeIdx::from_leaf(first_idx),
            first_len,
            idx_in_parent + 1,
            NodeIdx::from_leaf(second_idx),
            second_len,
        );

        let parent = self.inode(parent_idx);
        let diff = Length::diff(old_len, parent.len());

        self.bubble(parent_idx, maybe_split, diff, leaf_diff);

        (first_idx, second_idx)
    }

    #[inline]
    fn is_root(&self, idx: InodeIdx) -> bool {
        idx == self.root_idx
    }

    #[inline]
    fn last_leaf(&self) -> (LeafIdx<L>, ChildIdx) {
        let mut inode_idx = self.root_idx;

        loop {
            match self.inode(inode_idx).children() {
                Either::Internal(inode_idxs) => {
                    inode_idx = inode_idxs[inode_idxs.len() - 1];
                },

                Either::Leaf(leaf_idxs) => {
                    let child_idx = leaf_idxs.len() - 1;
                    return (leaf_idxs[child_idx], child_idx);
                },
            }
        }
    }

    #[inline(always)]
    fn leaf_mut(&mut self, idx: LeafIdx<L>) -> &mut L {
        self.lnode_mut(idx).value_mut()
    }

    #[inline]
    fn len_offset_of_child(
        &self,
        in_inode: InodeIdx,
        child_offset: ChildIdx,
    ) -> Length {
        let mut len = 0;

        match self.inode(in_inode).children() {
            Either::Internal(inode_idxs) => {
                for &idx in &inode_idxs[..child_offset] {
                    len += self.inode(idx).len();
                }
            },

            Either::Leaf(leaf_idxs) => {
                for &idx in &leaf_idxs[..child_offset] {
                    len += self.leaf(idx).len();
                }
            },
        }

        len
    }

    #[inline(always)]
    fn lnode(&self, idx: LeafIdx<L>) -> &Lnode<L> {
        &self.lnodes[idx.idx]
    }

    #[inline(always)]
    fn lnode_mut(&mut self, idx: LeafIdx<L>) -> &mut Lnode<L> {
        &mut self.lnodes[idx.idx]
    }

    /// Returns the index of the leaf after `leaf_idx` by only looking at its
    /// siblings. Returns `None` if the given leaf is the last leaf in its
    /// parent.
    #[inline]
    fn next_sibling(
        &self,
        leaf_idx: LeafIdx<L>,
        idx_in_parent: ChildIdx,
    ) -> Option<LeafIdx<L>> {
        let parent = self.inode(self.lnode(leaf_idx).parent());

        if idx_in_parent + 1 == parent.num_children() {
            None
        } else {
            Some(parent.child(idx_in_parent + 1).unwrap_leaf())
        }
    }

    /// Same as [`next_sibling()`] but filters out empty leaves.
    #[inline]
    fn next_non_empty_sibling(
        &self,
        leaf_idx: LeafIdx<L>,
        idx_in_parent: ChildIdx,
    ) -> Option<(LeafIdx<L>, ChildIdx)> {
        let parent = self.inode(self.lnode(leaf_idx).parent());

        let leaf_idxs = parent.children().unwrap_leaf();

        leaf_idxs[idx_in_parent + 1..].iter().enumerate().find_map(
            |(idx, &leaf_idx)| {
                (!self.leaf(leaf_idx).is_empty())
                    .then_some((leaf_idx, idx_in_parent + 1 + idx))
            },
        )
    }

    /// Returns the offset of the given inode in its parent, i.e. the sum of
    /// the lengths of all its siblings before it.
    #[inline]
    fn offset_of_internal_child(&self, inode_idx: InodeIdx) -> Length {
        let parent_idx = self.inode(inode_idx).parent();

        let siblings = self.inode(parent_idx).children().unwrap_internal();

        let mut offset = 0;

        for &idx in siblings {
            if idx == inode_idx {
                return offset;
            }
            offset += self.inode(idx).len();
        }

        unreachable!();
    }

    /// Returns the offset of the given leaf in its parent, i.e. the sum of the
    /// lengths of all its siblings before it.
    #[inline]
    fn offset_of_leaf_child(&self, leaf_idx: LeafIdx<L>) -> Length {
        let parent_idx = self.lnode(leaf_idx).parent();

        let siblings = self.inode(parent_idx).children().unwrap_leaf();

        let mut offset = 0;

        for &idx in siblings {
            if idx == leaf_idx {
                return offset;
            }
            offset += self.leaf(idx).len();
        }

        unreachable!();
    }

    /// Returns the index of the leaf before `leaf_idx` by only looking at its
    /// siblings. Returns `None` if the given leaf is the first leaf in its
    /// parent.
    #[inline]
    fn prev_sibling(
        &self,
        leaf_idx: LeafIdx<L>,
        idx_in_parent: ChildIdx,
    ) -> Option<LeafIdx<L>> {
        let parent = self.inode(self.lnode(leaf_idx).parent());

        if idx_in_parent == 0 {
            None
        } else {
            Some(parent.child(idx_in_parent - 1).unwrap_leaf())
        }
    }

    /// Same as [`prev_sibling()`] but filters out empty leaves.
    #[inline]
    fn prev_non_empty_sibling(
        &self,
        leaf_idx: LeafIdx<L>,
        idx_in_parent: ChildIdx,
    ) -> Option<(LeafIdx<L>, ChildIdx)> {
        let parent = self.inode(self.lnode(leaf_idx).parent());

        let leaf_idxs = parent.children().unwrap_leaf();

        leaf_idxs[..idx_in_parent].iter().rev().enumerate().find_map(
            |(idx, &leaf_idx)| {
                (!self.leaf(leaf_idx).is_empty())
                    .then_some((leaf_idx, idx_in_parent - 1 - idx))
            },
        )
    }

    /// Pushes an inode to the Gtree, returning its index.
    ///
    /// Note that the `parent` field of the given inode should be updated
    /// before calling this method.
    #[inline(always)]
    fn push_inode(
        &mut self,
        mut inode: Inode<ARITY, L>,
        parent: InodeIdx,
    ) -> InodeIdx {
        *inode.parent_mut() = parent;
        let idx = InodeIdx(self.inodes.len());
        assert!(!idx.is_dangling());
        inode.pushed(self, idx);
        self.inodes.push(inode);
        idx
    }

    /// Pushes a leaf node to the Gtree, returning its index.
    #[inline]
    fn push_leaf(&mut self, leaf: L, parent: InodeIdx) -> LeafIdx<L> {
        let idx = LeafIdx::new(self.lnodes.len());
        self.lnodes.push(Lnode::new(leaf, parent));
        idx
    }

    #[inline]
    fn root(&self) -> &Inode<ARITY, L> {
        self.inode(self.root_idx)
    }

    /// Called when the root has split into two inodes and a new root
    /// needs to be created.
    #[inline]
    fn root_has_split(&mut self, root_split: Inode<ARITY, L>) {
        let split_len = root_split.len();

        let split_idx = self.push_inode(root_split, InodeIdx::dangling());

        let new_root = Inode::from_two_internals(
            self.root_idx,
            split_idx,
            self.len(),
            split_len,
            InodeIdx::dangling(),
        );

        let new_root_idx = self.push_inode(new_root, InodeIdx::dangling());

        *self.root_mut().parent_mut() = new_root_idx;
        *self.inode_mut(split_idx).parent_mut() = new_root_idx;

        self.root_idx = new_root_idx;
    }

    #[cfg(feature = "encode")]
    #[inline(always)]
    pub(crate) fn root_idx(&self) -> InodeIdx {
        self.root_idx
    }

    #[inline]
    fn root_mut(&mut self) -> &mut Inode<ARITY, L> {
        self.inode_mut(self.root_idx)
    }

    #[inline]
    fn split_inode(
        &mut self,
        inode_idx: InodeIdx,
        at_offset: usize,
    ) -> Inode<ARITY, L> {
        let len = self.len_offset_of_child(inode_idx, at_offset);
        self.inode_mut(inode_idx).split(at_offset, len)
    }

    /// Returns a new, uninitialized Gtree.
    ///
    /// Once you have the first leaf makes sure to call [`initialize`] before
    /// calling any other methods on the Gtree.
    #[inline(always)]
    fn uninit() -> Self {
        Self::new(Vec::new(), Vec::new(), InodeIdx::dangling())
    }

    #[inline]
    fn update_child_idx_after_inserting_after(
        &self,
        old_parent: InodeIdx,
        old_idx: ChildIdx,
    ) -> ChildIdx {
        let parent = self.inode(old_parent);

        if parent.num_children() > old_idx {
            old_idx
        } else {
            old_idx - parent.num_children()
        }
    }

    /// TODO: docs
    #[inline]
    fn with_internal_mut_handle_parent<F, R>(
        &mut self,
        inode_idx: InodeIdx,
        fun: F,
    ) -> R
    where
        F: FnOnce(&mut Self) -> R,
    {
        debug_assert!(inode_idx != self.root_idx);

        let old_len = self.inode(inode_idx).len();

        let ret = fun(self);

        let inode = self.inode(inode_idx);

        let new_len = inode.len();

        let parent_idx = inode.parent();

        debug_assert!(!parent_idx.is_dangling());

        let parent = self.inode_mut(parent_idx);
        *parent.len_mut() -= old_len;
        *parent.len_mut() += new_len;

        ret
    }

    /// TODO: docs
    #[inline]
    fn with_internal_mut_handle_split<F, R>(
        &mut self,
        inode_idx: InodeIdx,
        idx_in_parent: usize,
        fun: F,
    ) -> (R, Option<Inode<ARITY, L>>)
    where
        F: FnOnce(&mut Self) -> (R, Option<Inode<ARITY, L>>),
    {
        let (ret, maybe_split) =
            self.with_internal_mut_handle_parent(inode_idx, fun);

        let split = maybe_split.and_then(|split| {
            let parent_idx = self.inode(inode_idx).parent();
            let len = split.len();
            let split_idx = self.push_inode(split, parent_idx);
            self.insert_in_inode(
                parent_idx,
                idx_in_parent + 1,
                NodeIdx::from_internal(split_idx),
                len,
            )
        });

        (ret, split)
    }

    /// TODO: docs
    #[inline]
    fn with_leaf_mut_handle_parent<F, T>(
        &mut self,
        leaf_idx: LeafIdx<L>,
        fun: F,
    ) -> T
    where
        F: FnOnce(&mut L) -> T,
    {
        let lnode = self.lnode_mut(leaf_idx);

        let leaf = lnode.value_mut();

        let old_len = leaf.len();

        let ret = fun(leaf);

        let new_len = leaf.len();

        let parent_idx = lnode.parent();

        let parent = self.inode_mut(parent_idx);
        *parent.len_mut() -= old_len;
        *parent.len_mut() += new_len;

        ret
    }

    /// TODO: docs
    #[allow(clippy::type_complexity)]
    #[inline]
    fn with_leaf_mut_handle_split<F>(
        &mut self,
        leaf_idx: LeafIdx<L>,
        idx_in_parent: usize,
        fun: F,
    ) -> (Option<LeafIdx<L>>, Option<LeafIdx<L>>, Option<Inode<ARITY, L>>)
    where
        F: FnOnce(&mut L) -> (Option<L>, Option<L>),
    {
        let parent_idx = self.lnode(leaf_idx).parent();

        let (left, right) = self.with_leaf_mut_handle_parent(leaf_idx, fun);

        let insert_at = idx_in_parent + 1;

        match (left, right) {
            (Some(left), Some(right)) => {
                let left_len = left.len();
                let left_idx = self.push_leaf(left, parent_idx);

                let right_len = right.len();
                let right_idx = self.push_leaf(right, parent_idx);

                let split = self.insert_two_in_inode(
                    parent_idx,
                    insert_at,
                    NodeIdx::from_leaf(left_idx),
                    left_len,
                    insert_at,
                    NodeIdx::from_leaf(right_idx),
                    right_len,
                );

                (Some(left_idx), Some(right_idx), split)
            },

            (Some(left), None) => {
                let len = left.len();
                let left_idx = self.push_leaf(left, parent_idx);

                let split = self.insert_in_inode(
                    parent_idx,
                    insert_at,
                    NodeIdx::from_leaf(left_idx),
                    len,
                );

                (Some(left_idx), None, split)
            },

            _ => (None, None, None),
        }
    }
}

impl<const N: usize, const M: usize, L: Leaf> PartialEq<Gtree<M, L>>
    for Gtree<N, L>
where
    L: PartialEq,
{
    #[inline]
    fn eq(&self, other: &Gtree<M, L>) -> bool {
        let mut this = self.leaves_from_first();
        let mut other = other.leaves_from_first();
        loop {
            match (this.next(), other.next()) {
                (None, None) => return true,
                (Some((_, this)), Some((_, other))) => {
                    if this != other {
                        return false;
                    }
                },
                _ => return false,
            }
        }
    }
}

/// The index inside the `children` array of a particular child of an internal
/// node.
type ChildIdx = usize;

/// An internal node of the Gtree.
#[derive(Clone)]
pub(crate) struct Inode<const ARITY: usize, L: Leaf> {
    /// The total len of this node, which is the sum of the lengths of
    /// all of its children.
    tot_len: Length,

    /// The index of this node's parent, or `InodeIdx::dangling()` if this is
    /// the root node.
    parent: InodeIdx,

    /// The number of children this inode is storing, which is always less than
    /// or equal to `ARITY`.
    num_children: usize,

    /// The indexes of this node's children in the Gtree. The first
    /// `num_children` are valid, while the rest are dangling.
    children: [NodeIdx<L>; ARITY],

    /// Whether `children` contains `LeafIdx<L>`s or `InodeIdx`s.
    has_leaves: bool,
}

impl<const ARITY: usize, L> PartialEq<Inode<ARITY, L>> for Inode<ARITY, L>
where
    L: Leaf,
{
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.tot_len == other.tot_len
            && self.parent == other.parent
            && self.num_children == other.num_children
            && self.has_leaves == other.has_leaves
            && self.children() == other.children()
    }
}

/// An index to either an internal node or a leaf node of the Gtree.
///
/// We use a union here to save space, since we know that an inode can only
/// store either internal nodes or leaf nodes, but not both.
///
/// This means that we can use a single boolean (the `has_leaves` field of
/// `Inode`) instead of storing the same tag for every single child of the
/// inode, like we would have to do if we used an enum.
union NodeIdx<L> {
    internal: InodeIdx,
    leaf: LeafIdx<L>,
}

impl<L> Copy for NodeIdx<L> {}

impl<L> Clone for NodeIdx<L> {
    #[inline]
    fn clone(&self) -> Self {
        *self
    }
}

impl<L> NodeIdx<L> {
    #[cfg(feature = "encode")]
    #[inline]
    const fn into_usize(self) -> usize {
        // SAFETY: both `InodeIdx` and `LeafIdx<L>` are basically `usize`s.
        unsafe { mem::transmute::<Self, usize>(self) }
    }

    #[inline]
    const fn dangling() -> Self {
        Self { internal: InodeIdx::dangling() }
    }

    #[inline]
    const fn from_internal(internal_idx: InodeIdx) -> Self {
        Self { internal: internal_idx }
    }

    #[inline]
    const fn from_leaf(leaf_idx: LeafIdx<L>) -> Self {
        Self { leaf: leaf_idx }
    }
}

#[derive(PartialEq)]
enum Either<I, L> {
    Internal(I),
    Leaf(L),
}

impl<I, L> Either<I, L> {
    #[inline]
    fn unwrap_internal(self) -> I {
        match self {
            Self::Internal(inode) => inode,
            Self::Leaf(_) => unreachable!(),
        }
    }

    #[inline]
    fn unwrap_leaf(self) -> L {
        match self {
            Self::Internal(_) => unreachable!(),
            Self::Leaf(leaf) => leaf,
        }
    }
}

impl<const ARITY: usize, L: Leaf> Inode<ARITY, L> {
    #[inline]
    fn child(&self, child_idx: ChildIdx) -> Either<InodeIdx, LeafIdx<L>> {
        let child = self.children[child_idx];

        if self.has_leaves {
            Either::Leaf(unsafe { child.leaf })
        } else {
            Either::Internal(unsafe { child.internal })
        }
    }

    #[inline]
    fn children(&self) -> Either<&[InodeIdx], &[LeafIdx<L>]> {
        let children = &self.children[..self.num_children];

        // SAFETY: `LeafIdx<L>` and `InodeIdx` have the same layout, so the
        // `NodeIdx` union also has the same layout and we can safely
        // transmute it into either of them.

        if self.has_leaves {
            let leaves =
                unsafe { mem::transmute::<_, &[LeafIdx<L>]>(children) };
            Either::Leaf(leaves)
        } else {
            let inodes = unsafe { mem::transmute::<_, &[InodeIdx]>(children) };
            Either::Internal(inodes)
        }
    }

    /// Creates a new internal node containing the given leaf.
    #[inline]
    fn from_leaf(leaf: LeafIdx<L>, len: Length, parent: InodeIdx) -> Self {
        let mut children = [NodeIdx::dangling(); ARITY];
        children[0] = NodeIdx::from_leaf(leaf);
        Self {
            children,
            parent,
            tot_len: len,
            has_leaves: true,
            num_children: 1,
        }
    }

    /// Creates a new internal node containing the two given inodes.
    #[inline]
    fn from_two_internals(
        first: InodeIdx,
        second: InodeIdx,
        first_len: Length,
        second_len: Length,
        parent: InodeIdx,
    ) -> Self {
        let mut children = [NodeIdx::dangling(); ARITY];
        children[0] = NodeIdx::from_internal(first);
        children[1] = NodeIdx::from_internal(second);

        Self {
            children,
            parent,
            tot_len: first_len + second_len,
            has_leaves: false,
            num_children: 2,
        }
    }

    /// Returns the index of the child matching the given `InodeIdx`.
    ///
    /// Panics if this inode contains leaf nodes or if it doesn't contain the
    /// given `InodeIdx`.
    #[inline]
    fn idx_of_internal_child(&self, inode_idx: InodeIdx) -> ChildIdx {
        self.children()
            .unwrap_internal()
            .iter()
            .enumerate()
            .find_map(|(i, &idx)| (idx == inode_idx).then_some(i))
            .expect("this inode does not contain the given inode idx")
    }

    /// Returns the index of the child matching the given `LeafIdx<L>`.
    ///
    /// Panics if this inode contains inodes or if it doesn't contain the given
    /// `LeafIdx<L>`.
    #[inline]
    fn idx_of_leaf_child(&self, leaf_idx: LeafIdx<L>) -> ChildIdx {
        self.children()
            .unwrap_leaf()
            .iter()
            .enumerate()
            .find_map(|(i, &idx)| (idx == leaf_idx).then_some(i))
            .expect("this inode does not contain the given leaf idx")
    }

    /// Inserts two children into this inode at the given offsets.
    ///
    /// Panics if this doesn't have enough space to store two more children, if
    /// the second offset is less than the first, or if one of the offsets is
    /// out of bounds.
    #[inline]
    fn insert_two(
        &mut self,
        first_offset: usize,
        first_child: NodeIdx<L>,
        first_len: Length,
        second_offset: usize,
        second_child: NodeIdx<L>,
        second_len: Length,
    ) {
        debug_assert!(first_offset <= second_offset);
        debug_assert!(second_offset <= self.num_children());
        debug_assert!(Self::max_children() - self.num_children() >= 2);

        self.insert(first_offset, first_child, first_len);
        self.insert(second_offset + 1, second_child, second_len);
    }

    /// Inserts a child into this inode at the given offset.
    ///
    /// Panics if this inode is already full or if the offset is out of bounds.
    #[inline]
    fn insert(
        &mut self,
        at_offset: usize,
        child: NodeIdx<L>,
        child_len: Length,
    ) {
        debug_assert!(at_offset <= self.num_children());
        debug_assert!(!self.is_full());
        crate::insert_in_slice(&mut self.children, child, at_offset);
        self.tot_len += child_len;
        self.num_children += 1;
    }

    #[inline]
    fn is_full(&self) -> bool {
        self.num_children() == Self::max_children()
    }

    #[inline(always)]
    fn len(&self) -> Length {
        self.tot_len
    }

    #[inline(always)]
    fn len_mut(&mut self) -> &mut Length {
        &mut self.tot_len
    }

    #[inline]
    const fn max_children() -> usize {
        ARITY
    }

    #[inline]
    const fn min_children() -> usize {
        ARITY / 2
    }

    #[inline(always)]
    fn num_children(&self) -> usize {
        self.num_children
    }

    #[inline(always)]
    fn parent(&self) -> InodeIdx {
        self.parent
    }

    #[inline(always)]
    fn parent_mut(&mut self) -> &mut InodeIdx {
        &mut self.parent
    }

    /// Called when this inode has been pushed to the Gtree and has been
    /// assigned the given `InodeIdx`.
    ///
    /// This is used to update the parent field of all the children of this
    /// inode.
    #[inline]
    fn pushed(&mut self, gtree: &mut Gtree<ARITY, L>, to_idx: InodeIdx) {
        match self.children() {
            Either::Internal(internal_idxs) => {
                for &idx in internal_idxs {
                    let child_inode = gtree.inode_mut(idx);
                    *child_inode.parent_mut() = to_idx;
                }
            },

            Either::Leaf(leaf_idxs) => {
                for &idx in leaf_idxs {
                    let child_lnode = gtree.lnode_mut(idx);
                    *child_lnode.parent_mut() = to_idx;
                }
            },
        }
    }

    /// Split this inode at the given offset, returning the right side of the
    /// split.
    ///
    /// The `new_len` parameter is the sum of the lengths of the first
    /// `at_offset` children.
    #[inline]
    fn split(&mut self, at_offset: usize, new_len: Length) -> Self {
        let num_children = self.num_children() - at_offset;

        let mut children = [NodeIdx::dangling(); ARITY];
        children[..num_children]
            .copy_from_slice(&self.children[at_offset..self.num_children()]);

        let len = self.tot_len - new_len;

        self.tot_len = new_len;

        self.num_children = at_offset;

        Self {
            children,
            has_leaves: self.has_leaves,
            num_children,
            parent: InodeIdx::dangling(),
            tot_len: len,
        }
    }
}

/// A leaf node of the Gtree.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Lnode<Leaf> {
    /// The value of this leaf node.
    value: Leaf,

    /// The index of this leaf node's parent.
    parent: InodeIdx,
}

impl<L: Leaf> Lnode<L> {
    #[inline(always)]
    fn len(&self) -> Length {
        self.value.len()
    }

    #[inline(always)]
    pub(crate) fn new(value: L, parent: InodeIdx) -> Self {
        Self { value, parent }
    }

    #[inline(always)]
    fn parent(&self) -> InodeIdx {
        self.parent
    }

    #[inline(always)]
    fn parent_mut(&mut self) -> &mut InodeIdx {
        &mut self.parent
    }

    #[inline(always)]
    fn value(&self) -> &L {
        &self.value
    }

    #[inline(always)]
    fn value_mut(&mut self) -> &mut L {
        &mut self.value
    }
}

mod insert {
    use super::*;

    #[allow(clippy::type_complexity)]
    #[inline]
    pub(super) fn insert_at_offset<const N: usize, L, F>(
        gtree: &mut Gtree<N, L>,
        in_inode: InodeIdx,
        mut leaf_offset: Length,
        at_offset: Length,
        insert_with: F,
    ) -> ((Option<LeafIdx<L>>, Option<LeafIdx<L>>), Option<Inode<N, L>>)
    where
        L: Leaf,
        F: FnOnce(&mut L, Length) -> (Option<L>, Option<L>),
    {
        let (child_idx, offset) =
            gtree.child_at_offset(in_inode, at_offset - leaf_offset);

        leaf_offset += offset;

        match gtree.inode(in_inode).child(child_idx) {
            Either::Internal(next_idx) => gtree
                .with_internal_mut_handle_split(
                    next_idx,
                    child_idx,
                    |gtree| {
                        insert_at_offset(
                            gtree,
                            next_idx,
                            leaf_offset,
                            at_offset,
                            insert_with,
                        )
                    },
                ),

            Either::Leaf(leaf_idx) => {
                let (inserted_idx, split_idx, split) = gtree
                    .with_leaf_mut_handle_split(leaf_idx, child_idx, |leaf| {
                        insert_with(leaf, at_offset - leaf_offset)
                    });

                gtree.cursor = if let Some(idx) = inserted_idx {
                    let leaf_len = gtree.leaf(leaf_idx).len();

                    let idx_in_parent = gtree
                        .update_child_idx_after_inserting_after(
                            in_inode,
                            child_idx + 1,
                        );

                    Some(Cursor::new(
                        idx,
                        leaf_offset + leaf_len,
                        idx_in_parent,
                    ))
                } else {
                    let idx_in_parent = gtree
                        .update_child_idx_after_inserting_after(
                            in_inode, child_idx,
                        );

                    Some(Cursor::new(leaf_idx, leaf_offset, idx_in_parent))
                };

                ((inserted_idx, split_idx), split)
            },
        }
    }
}

mod delete {
    use super::*;

    #[allow(clippy::type_complexity)]
    pub(super) fn delete_range_in_inode<
        const N: usize,
        L,
        BeforeDelete,
        DelFrom,
        DelUpTo,
    >(
        gtree: &mut Gtree<N, L>,
        idx: InodeIdx,
        range: Range<Length>,
        before_delete: &mut BeforeDelete,
        del_from: DelFrom,
        del_up_to: DelUpTo,
    ) -> ((Option<LeafIdx<L>>, Option<LeafIdx<L>>), Option<Inode<N, L>>)
    where
        L: Leaf + Delete,
        BeforeDelete: FnMut(&L),
        DelFrom: FnOnce(&mut L, Length) -> Option<L>,
        DelUpTo: FnOnce(&mut L, Length) -> Option<L>,
    {
        let mut idx_start = 0;
        let mut leaf_idx_start = None;
        let mut extra_from_start = None;

        let mut idx_end = 0;
        let mut leaf_idx_end = None;
        let mut extra_from_end = None;

        let mut idxs = 0..gtree.inode(idx).num_children();
        let mut offset = 0;

        for child_idx in idxs.by_ref() {
            let child_measure = gtree.child_measure(idx, child_idx);

            offset += child_measure;

            if offset > range.start {
                offset -= child_measure;

                idx_start = child_idx;

                match gtree.inode(idx).child(child_idx) {
                    Either::Internal(inode_idx) => {
                        let (leaf_idx, split) = gtree
                            .with_internal_mut_handle_parent(
                                inode_idx,
                                |gtree| {
                                    delete_from(
                                        gtree,
                                        inode_idx,
                                        range.start - offset,
                                        before_delete,
                                        del_from,
                                    )
                                },
                            );

                        leaf_idx_start = leaf_idx;

                        if let Some(extra) = split {
                            let len = extra.len();
                            let inode_idx = gtree.push_inode(extra, idx);
                            let node_idx = NodeIdx::from_internal(inode_idx);
                            extra_from_start = Some((node_idx, len));
                        }
                    },

                    Either::Leaf(leaf_idx) => {
                        let split = gtree.with_leaf_mut_handle_parent(
                            leaf_idx,
                            |leaf| {
                                before_delete(leaf);
                                del_from(leaf, range.start - offset)
                            },
                        );

                        if let Some(split) = split {
                            let len = split.len();
                            let leaf_idx = gtree.push_leaf(split, idx);
                            leaf_idx_start = Some(leaf_idx);
                            let node_idx = NodeIdx::from_leaf(leaf_idx);
                            extra_from_start = Some((node_idx, len));
                        }
                    },
                }

                offset += child_measure;

                break;
            }
        }

        for child_idx in idxs {
            let child_measure = gtree.child_measure(idx, child_idx);

            offset += child_measure;

            if offset >= range.end {
                offset -= child_measure;

                idx_end = child_idx;

                match gtree.inode(idx).child(child_idx) {
                    Either::Internal(inode_idx) => {
                        let (leaf_idx, split) = gtree
                            .with_internal_mut_handle_parent(
                                inode_idx,
                                |gtree| {
                                    delete_up_to(
                                        gtree,
                                        inode_idx,
                                        range.end - offset,
                                        before_delete,
                                        del_up_to,
                                    )
                                },
                            );

                        leaf_idx_end = leaf_idx;

                        if let Some(extra) = split {
                            let len = extra.len();
                            let inode_idx = gtree.push_inode(extra, idx);
                            let node_idx = NodeIdx::from_internal(inode_idx);
                            extra_from_end = Some((node_idx, len));
                        }
                    },

                    Either::Leaf(leaf_idx) => {
                        let split = gtree.with_leaf_mut_handle_parent(
                            leaf_idx,
                            |leaf| {
                                before_delete(leaf);
                                del_up_to(leaf, range.end - offset)
                            },
                        );

                        if let Some(split) = split {
                            let len = split.len();
                            let leaf_idx = gtree.push_leaf(split, idx);

                            leaf_idx_end = Some(leaf_idx);

                            let node_idx = NodeIdx::from_leaf(leaf_idx);
                            extra_from_end = Some((node_idx, len));
                        }
                    },
                }

                break;
            } else {
                gtree.delete_child(idx, child_idx, before_delete);
            }
        }

        let start_offset = idx_start + 1;

        let end_offset = idx_end + 1;

        let split = match (extra_from_start, extra_from_end) {
            (Some((start, start_len)), Some((end, end_len))) => gtree
                .insert_two_in_inode(
                    idx,
                    start_offset,
                    start,
                    start_len,
                    end_offset,
                    end,
                    end_len,
                ),

            (Some((start, len)), None) => {
                gtree.insert_in_inode(idx, start_offset, start, len)
            },

            (None, Some((end, len))) => {
                gtree.insert_in_inode(idx, end_offset, end, len)
            },

            (None, None) => None,
        };

        ((leaf_idx_start, leaf_idx_end), split)
    }

    fn delete_from<const N: usize, L, BeforeDelete, DelFrom>(
        gtree: &mut Gtree<N, L>,
        inode_idx: InodeIdx,
        mut from: Length,
        before_delete: &mut BeforeDelete,
        del_from: DelFrom,
    ) -> (Option<LeafIdx<L>>, Option<Inode<N, L>>)
    where
        L: Leaf + Delete,
        BeforeDelete: FnMut(&L),
        DelFrom: FnOnce(&mut L, Length) -> Option<L>,
    {
        let len = gtree.inode(inode_idx).num_children();

        let mut offset = 0;

        for child_idx in 0..len {
            let child_measure = gtree.child_measure(inode_idx, child_idx);

            offset += child_measure;

            if offset > from {
                for child_idx in child_idx + 1..len {
                    gtree.delete_child(inode_idx, child_idx, before_delete);
                }

                offset -= child_measure;

                from -= offset;

                return match gtree.inode(inode_idx).child(child_idx) {
                    Either::Internal(next_idx) => gtree
                        .with_internal_mut_handle_split(
                            next_idx,
                            child_idx,
                            |gtree| {
                                delete_from(
                                    gtree,
                                    next_idx,
                                    from,
                                    before_delete,
                                    del_from,
                                )
                            },
                        ),

                    Either::Leaf(leaf_idx) => {
                        let (idx, _none, split) = gtree
                            .with_leaf_mut_handle_split(
                                leaf_idx,
                                child_idx,
                                |leaf| {
                                    before_delete(leaf);
                                    (del_from(leaf, from), None)
                                },
                            );

                        (idx, split)
                    },
                };
            }
        }

        unreachable!();
    }

    fn delete_up_to<const N: usize, L, BeforeDelete, DelUpTo>(
        gtree: &mut Gtree<N, L>,
        idx: InodeIdx,
        mut up_to: Length,
        before_delete: &mut BeforeDelete,
        del_up_to: DelUpTo,
    ) -> (Option<LeafIdx<L>>, Option<Inode<N, L>>)
    where
        L: Leaf + Delete,
        BeforeDelete: FnMut(&L),
        DelUpTo: FnOnce(&mut L, Length) -> Option<L>,
    {
        let mut offset = 0;

        for child_idx in 0..gtree.inode(idx).num_children() {
            let child_measure = gtree.child_measure(idx, child_idx);

            offset += child_measure;

            if offset >= up_to {
                offset -= child_measure;

                up_to -= offset;

                return match gtree.inode(idx).child(child_idx) {
                    Either::Internal(next_idx) => gtree
                        .with_internal_mut_handle_split(
                            next_idx,
                            child_idx,
                            |gtree| {
                                delete_up_to(
                                    gtree,
                                    next_idx,
                                    up_to,
                                    before_delete,
                                    del_up_to,
                                )
                            },
                        ),

                    Either::Leaf(leaf_idx) => {
                        let (idx, _none, split) = gtree
                            .with_leaf_mut_handle_split(
                                leaf_idx,
                                child_idx,
                                |leaf| {
                                    before_delete(leaf);
                                    (del_up_to(leaf, up_to), None)
                                },
                            );

                        (idx, split)
                    },
                };
            } else {
                gtree.delete_child(idx, child_idx, before_delete);
            }
        }

        unreachable!();
    }
}

pub use debug::{DebugAsBtree, DebugAsSelf};

mod debug {
    //! Debug implementations for types in the outer module.
    //!
    //! Placed here to avoid cluttering the main module.

    use core::fmt::{Formatter, Result as FmtResult};

    use super::*;

    /// Custom Debug implementation which always prints the output in a single
    /// line even when the alternate flag (`{:#?}`) is enabled.
    impl Debug for InodeIdx {
        fn fmt(&self, f: &mut Formatter) -> FmtResult {
            write!(f, "InodeIdx({})", self.0)
        }
    }

    /// Custom Debug implementation which always prints the output in a single
    /// line even when the alternate flag (`{:#?}`) is enabled.
    impl<L: Leaf> Debug for LeafIdx<L> {
        fn fmt(&self, f: &mut Formatter) -> FmtResult {
            let type_name = core::any::type_name::<L>();
            let type_name_wo_path = type_name.split("::").last().unwrap();
            write!(f, "LeafIdx<{}>({})", type_name_wo_path, self.idx)
        }
    }

    impl<const N: usize, L: Leaf> Debug for Inode<N, L> {
        fn fmt(&self, f: &mut Formatter) -> FmtResult {
            if !self.parent().is_dangling() {
                write!(f, "{:?} <- ", self.parent())?;
            }

            write!(f, "{:?} @ ", self.len())?;

            let mut dbg = f.debug_list();

            match self.children() {
                Either::Internal(inode_idxs) => {
                    dbg.entries(inode_idxs).finish()
                },
                Either::Leaf(leaf_idxs) => dbg.entries(leaf_idxs).finish(),
            }
        }
    }

    impl<const ARITY: usize, L: Leaf> Debug for Gtree<ARITY, L> {
        fn fmt(&self, f: &mut Formatter) -> FmtResult {
            self.debug_as_btree().fmt(f)
        }
    }

    struct DebugAsDisplay<'a>(&'a dyn core::fmt::Display);

    impl Debug for DebugAsDisplay<'_> {
        fn fmt(&self, f: &mut Formatter) -> FmtResult {
            core::fmt::Display::fmt(self.0, f)
        }
    }

    /// A type providing a Debug implementation for `Gtree` which prints
    /// `Inode`s and `Lnode`s sequentially like they are stored in memory.
    pub struct DebugAsSelf<'a, const N: usize, L: Leaf>(
        pub(super) &'a Gtree<N, L>,
    );

    impl<const N: usize, L: Leaf> Debug for DebugAsSelf<'_, N, L> {
        fn fmt(&self, f: &mut Formatter) -> FmtResult {
            let gtree = self.0;

            let debug_inodes = DebugInodesSequentially {
                inodes: gtree.inodes.as_slice(),
                root_idx: gtree.root_idx.0,
            };

            let debug_lnodes =
                DebugLnodesSequentially(gtree.lnodes.as_slice());

            let inodes = DebugAsDisplay(&"inodes");
            let lnodes = DebugAsDisplay(&"lnodes");

            f.debug_map()
                .entry(&inodes, &debug_inodes)
                .entry(&lnodes, &debug_lnodes)
                .finish()
        }
    }

    struct DebugInodesSequentially<'a, const N: usize, L: Leaf> {
        inodes: &'a [Inode<N, L>],
        root_idx: usize,
    }

    impl<const N: usize, L: Leaf> Debug for DebugInodesSequentially<'_, N, L> {
        fn fmt(&self, f: &mut Formatter) -> FmtResult {
            struct Key {
                idx: usize,
                is_root: bool,
            }

            impl Debug for Key {
                fn fmt(&self, f: &mut Formatter) -> FmtResult {
                    let prefix = if self.is_root { "R -> " } else { "" };
                    write!(f, "{prefix}{}", self.idx)
                }
            }

            let entries =
                self.inodes.iter().enumerate().map(|(idx, inode)| {
                    let key = Key { idx, is_root: idx == self.root_idx };
                    (key, inode)
                });

            f.debug_map().entries(entries).finish()
        }
    }

    struct DebugLnodesSequentially<'a, L: Leaf>(&'a [Lnode<L>]);

    impl<L: Leaf> Debug for DebugLnodesSequentially<'_, L> {
        fn fmt(&self, f: &mut Formatter) -> FmtResult {
            f.debug_map()
                .entries(self.0.iter().map(Lnode::value).enumerate())
                .finish()
        }
    }

    /// A type providing a Debug implementation for `Gtree` which prints it as
    /// the equivalent Btree, highlighting the parent-child relationships
    /// between the inodes and the leaf nodes.
    ///
    /// Note that this is not how the tree is actually stored in memory. If you
    /// want to see the actual memory layout, use `DebugAsSelf`.
    pub struct DebugAsBtree<'a, const N: usize, L: Leaf> {
        pub(super) gtree: &'a Gtree<N, L>,
        pub(super) inode_idx: InodeIdx,
    }

    impl<const N: usize, L: Leaf> Debug for DebugAsBtree<'_, N, L> {
        fn fmt(&self, f: &mut Formatter) -> FmtResult {
            fn print_inode_as_tree<const N: usize, L: Leaf>(
                gtree: &Gtree<N, L>,
                inode_idx: InodeIdx,
                shifts: &mut String,
                ident: &str,
                last_shift_byte_len: usize,
                f: &mut Formatter,
            ) -> FmtResult {
                let inode = gtree.inode(inode_idx);

                writeln!(
                    f,
                    "{}{}{:?}",
                    &shifts[..shifts.len() - last_shift_byte_len],
                    ident,
                    inode.len()
                )?;

                let is_last = |idx: usize| idx + 1 == inode.num_children();

                let ident = |idx: usize| {
                    if is_last(idx) {
                        "└── "
                    } else {
                        "├── "
                    }
                };

                let shift = |idx: usize| {
                    if is_last(idx) {
                        "    "
                    } else {
                        "│   "
                    }
                };

                match inode.children() {
                    Either::Internal(inode_idxs) => {
                        for (i, &inode_idx) in inode_idxs.iter().enumerate() {
                            let shift = shift(i);
                            shifts.push_str(shift);
                            let ident = ident(i);
                            print_inode_as_tree(
                                gtree,
                                inode_idx,
                                shifts,
                                ident,
                                shift.len(),
                                f,
                            )?;
                            shifts.truncate(shifts.len() - shift.len());
                        }
                    },

                    Either::Leaf(leaf_idxs) => {
                        for (i, &leaf_idx) in leaf_idxs.iter().enumerate() {
                            if let Some(cursor) = gtree.cursor {
                                if cursor.leaf_idx == leaf_idx {
                                    writeln!(
                                        f,
                                        "{}│\n{}│ -> cursor @ {:?}\n{}│",
                                        shifts, shifts, cursor.offset, shifts,
                                    )?;
                                }
                            }

                            let ident = ident(i);
                            let lnode = gtree.lnode(leaf_idx);
                            writeln!(
                                f,
                                "{}{}{:#?}",
                                &shifts,
                                ident,
                                &lnode.value()
                            )?;
                        }
                    },
                }

                Ok(())
            }

            writeln!(f)?;

            print_inode_as_tree(
                self.gtree,
                self.inode_idx,
                &mut String::new(),
                "",
                0,
                f,
            )
        }
    }
}

pub use iter::{Leaves, Siblings};

mod iter {
    use super::*;

    #[derive(Debug)]
    pub struct Leaves<'a, const N: usize, L: Leaf> {
        gtree: &'a Gtree<N, L>,
        path: Vec<(InodeIdx, ChildIdx)>,
        current_leaves: &'a [LeafIdx<L>],
    }

    impl<'a, const N: usize, L: Leaf> Leaves<'a, N, L> {
        #[inline]
        pub(super) fn new<const INCLUDE_START: bool>(
            gtree: &'a Gtree<N, L>,
            start_idx: LeafIdx<L>,
        ) -> Self {
            let (mut inode_idx, current_leaves) = {
                let parent_idx = gtree.lnode(start_idx).parent();
                let parent = gtree.inode(parent_idx);
                let child_idx = parent.idx_of_leaf_child(start_idx);
                let leaves = parent.children().unwrap_leaf();
                let idx =
                    if INCLUDE_START { child_idx } else { child_idx + 1 };
                (parent_idx, &leaves[idx..])
            };

            let mut path = Vec::new();

            while !gtree.is_root(inode_idx) {
                let parent_idx = gtree.inode(inode_idx).parent();
                let child_idx = gtree.idx_of_inode_in_parent(inode_idx);
                path.insert(0, (parent_idx, child_idx));
                inode_idx = parent_idx;
            }

            Self { gtree, path, current_leaves }
        }
    }

    impl<'a, const N: usize, L: Leaf> Iterator for Leaves<'a, N, L> {
        type Item = (LeafIdx<L>, &'a L);

        #[inline]
        fn next(&mut self) -> Option<Self::Item> {
            if let Some((&first, rest)) = self.current_leaves.split_first() {
                let leaf = self.gtree.leaf(first);
                self.current_leaves = rest;
                Some((first, leaf))
            } else {
                while let Some((idx, child_idx)) = self.path.last_mut() {
                    *child_idx += 1;
                    if *child_idx < self.gtree.inode(*idx).num_children() {
                        break;
                    }
                    self.path.pop();
                }

                let &(last_idx, child_idx) = self.path.last()?;

                let mut idx = self
                    .gtree
                    .inode(last_idx)
                    .child(child_idx)
                    .unwrap_internal();

                loop {
                    match self.gtree.inode(idx).children() {
                        Either::Internal(inode_idxs) => {
                            self.path.push((idx, 0));
                            idx = inode_idxs[0];
                        },

                        Either::Leaf(leaf_idxs) => {
                            self.current_leaves = leaf_idxs;
                            break;
                        },
                    }
                }

                self.next()
            }
        }
    }

    #[derive(Debug)]
    pub struct Siblings<'a, const N: usize, L: Leaf> {
        gtree: &'a Gtree<N, L>,
        leaf_idxs: &'a [LeafIdx<L>],
    }

    impl<'a, const N: usize, L: Leaf> Siblings<'a, N, L> {
        #[inline]
        pub(super) fn new<const INCLUDE_START: bool>(
            gtree: &'a Gtree<N, L>,
            start_idx: LeafIdx<L>,
        ) -> Self {
            let parent_idx = gtree.lnode(start_idx).parent();
            let parent = gtree.inode(parent_idx);
            let child_idx = parent.idx_of_leaf_child(start_idx);
            let siblings = parent.children().unwrap_leaf();
            let idx = if INCLUDE_START { child_idx } else { child_idx + 1 };
            Self { gtree, leaf_idxs: &siblings[idx..] }
        }
    }

    impl<'a, const N: usize, L: Leaf> Iterator for Siblings<'a, N, L> {
        type Item = (LeafIdx<L>, &'a L);

        #[inline]
        fn next(&mut self) -> Option<Self::Item> {
            let (&first, rest) = self.leaf_idxs.split_first()?;
            let leaf = self.gtree.leaf(first);
            self.leaf_idxs = rest;
            Some((first, leaf))
        }
    }
}

#[cfg(feature = "encode")]
pub(crate) mod encode {
    use super::*;
    use crate::encode::{BoolDecodeError, Decode, Encode, IntDecodeError};

    impl Encode for InodeIdx {
        #[inline]
        fn encode(&self, buf: &mut Vec<u8>) {
            self.0.encode(buf);
        }
    }

    impl Decode for InodeIdx {
        type Value = Self;

        type Error = IntDecodeError;

        #[inline]
        fn decode(buf: &[u8]) -> Result<(Self, &[u8]), Self::Error> {
            let (idx, rest) = usize::decode(buf)?;
            Ok((Self(idx), rest))
        }
    }

    impl<L> Encode for LeafIdx<L> {
        #[inline]
        fn encode(&self, buf: &mut Vec<u8>) {
            self.idx.encode(buf);
        }
    }

    impl<L> Decode for LeafIdx<L> {
        type Value = Self;

        type Error = IntDecodeError;

        #[inline]
        fn decode(buf: &[u8]) -> Result<(Self, &[u8]), Self::Error> {
            let (idx, rest) = usize::decode(buf)?;
            Ok((Self::new(idx), rest))
        }
    }

    impl<L> Encode for NodeIdx<L> {
        #[inline]
        fn encode(&self, buf: &mut Vec<u8>) {
            self.into_usize().encode(buf);
        }
    }

    impl<L> Decode for NodeIdx<L> {
        type Value = Self;

        type Error = IntDecodeError;

        #[inline]
        fn decode(buf: &[u8]) -> Result<(Self, &[u8]), Self::Error> {
            let (idx, rest) = usize::decode(buf)?;
            Ok((Self::from_internal(InodeIdx(idx)), rest))
        }
    }

    impl<const N: usize, L: Leaf> Encode for Inode<N, L>
    where
        Length: Encode,
    {
        #[inline]
        fn encode(&self, buf: &mut Vec<u8>) {
            self.tot_len.encode(buf);
            self.parent.encode(buf);
            self.num_children.encode(buf);
            self.has_leaves.encode(buf);

            match self.children() {
                Either::Internal(inode_idxs) => {
                    for idx in inode_idxs {
                        idx.encode(buf);
                    }
                },

                Either::Leaf(leaf_idxs) => {
                    for idx in leaf_idxs {
                        idx.encode(buf);
                    }
                },
            }
        }
    }

    pub(crate) enum InodeDecodeError {
        Bool(BoolDecodeError),
        Int(IntDecodeError),
    }

    impl From<BoolDecodeError> for InodeDecodeError {
        #[inline]
        fn from(err: BoolDecodeError) -> Self {
            Self::Bool(err)
        }
    }

    impl From<IntDecodeError> for InodeDecodeError {
        #[inline]
        fn from(err: IntDecodeError) -> Self {
            Self::Int(err)
        }
    }

    impl core::fmt::Display for InodeDecodeError {
        fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
            let err: &dyn core::fmt::Display = match self {
                Self::Bool(err) => err,
                Self::Int(err) => err,
            };

            write!(f, "Inode couldn't be decoded: {err}")
        }
    }

    impl<const N: usize, L: Leaf> Decode for Inode<N, L>
    where
        Length: Decode,
    {
        type Value = Self;

        type Error = InodeDecodeError;

        #[inline]
        fn decode(buf: &[u8]) -> Result<(Self, &[u8]), Self::Error> {
            let (tot_len, buf) = Length::decode(buf)?;
            let (parent, buf) = InodeIdx::decode(buf)?;
            let (num_children, buf) = usize::decode(buf)?;
            let (has_leaves, mut buf) = bool::decode(buf)?;
            let mut children = [NodeIdx::dangling(); N];
            for idx in &mut children[..num_children] {
                let (node_idx, new_buf) = NodeIdx::decode(buf)?;
                *idx = node_idx;
                buf = new_buf;
            }
            let this =
                Self { tot_len, parent, num_children, has_leaves, children };
            Ok((this, buf))
        }
    }
}
