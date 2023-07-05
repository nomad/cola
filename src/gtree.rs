use core::fmt::Debug;
use core::marker::PhantomData;
use core::mem;
use core::ops::{Add, AddAssign, Sub, SubAssign};

use crate::{Range, RangeExt};

/// A trait used to express the length of a node.
pub trait Length:
    Debug
    + Copy
    + Add<Self, Output = Self>
    + AddAssign<Self>
    + Sub<Self, Output = Self>
    + SubAssign<Self>
    + Ord
{
    /// A type used to represent the difference between two lengths.
    type Diff: Debug + Copy;

    /// Creates a diff which, when applied to `old_len`, produces `new_len`.
    fn diff(old_len: Self, new_len: Self) -> Self::Diff;

    /// Applies a diff to `self`.
    fn apply_diff(&mut self, patch: Self::Diff);

    /// Returns the identity element of this length wrt addition and
    /// subtraction.
    fn zero() -> Self;

    /// Returns `true` if the length is zero.
    #[inline]
    fn is_zero(self) -> bool {
        self == Self::zero()
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
    /// The method returns `None` if the join operation succeeded and
    /// `Some(other)` if it failed.
    #[allow(unused_variables)]
    fn append(&mut self, other: Self) -> Option<Self> {
        None
    }

    /// Try to prepend `other` to the beginning of `self`.
    ///
    /// The method returns `None` if the join operation succeeded and
    /// `Some(other)` if it failed.
    #[allow(unused_variables)]
    fn prepend(&mut self, other: Self) -> Option<Self> {
        None
    }
}

/// A trait implemented by the leaf nodes of a Gtree.
///
/// This is mostly just a supertrait of several other traits, but it's useful
/// to have a single trait to use as the bound for the leaves of a Gtree.
pub trait Leaf: Debug + Join {
    type Length: Length;

    fn len(&self) -> Self::Length;

    #[inline]
    fn is_empty(&self) -> bool {
        self.len().is_zero()
    }
}

/// Statically checks that `InodeIdx` and `LeafIdx` have identical size and
/// alignment.
///
/// This invariant is required by [`Inode::children()`] to safely transmute
/// `NodeIdx` slices into `InodeIdx` and `LeafIdx` slices.
const _NODE_IDX_LAYOUT_CHECK: usize = {
    use core::alloc::Layout;
    let i_idx = Layout::new::<InodeIdx>();
    let l_idx = Layout::new::<LeafIdx<()>>();
    (i_idx.size() == l_idx.size() && i_idx.align() == l_idx.align()) as usize
        - 1
};

/// A grow-only, self-balancing tree.
///
/// A Gtree is a tree data structure that has very similar invariants and
/// performance characteristics to those of a Btree, but differs in the way in
/// which the internal and leaf nodes are stored in memory.
///
/// TODO: finish describing the data structure.
#[derive(Clone)]
pub struct Gtree<const ARITY: usize, L: Leaf> {
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
struct InodeIdx(usize);

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
    const fn dangling() -> Self {
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

/// A stable identifier for a particular leaf of the Gtree.
#[derive(Eq)]
pub struct LeafIdx<L> {
    idx: usize,
    _pd: PhantomData<L>,
}

impl<L> LeafIdx<L> {
    /// TODO: docs
    #[inline]
    pub const fn dangling() -> Self {
        Self::new(usize::MAX)
    }

    #[inline]
    const fn new(idx: usize) -> Self {
        Self { idx, _pd: PhantomData }
    }
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

/// A cursor into the Gtree.
///
/// The name comes from its ability to identify a particular position between
/// two leaf nodes in the tree, much like a line cursor identifies a position
/// between two characters in a text editor.
#[derive(PartialEq, Eq)]
struct Cursor<L: Leaf> {
    /// The index of the leaf node that comes *after* the cursor.
    leaf_idx: LeafIdx<L>,

    /// The offset of `self.leaf_idx` in the Gtree, *without* taking into
    /// account the length of the leaf node itself.
    offset: L::Length,

    /// The child index of `self.leaf_idx` in its parent.
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
    fn new(
        leaf_idx: LeafIdx<L>,
        offset: L::Length,
        child_idx: ChildIdx,
    ) -> Self {
        Self { leaf_idx, offset, child_idx }
    }
}

// Public API.
impl<const ARITY: usize, L: Leaf> Gtree<ARITY, L> {
    /// Asserts the invariants of the Gtree.
    ///
    /// If this method returns without panicking, then the Gtree is in a
    /// consistent state.
    ///
    /// This is mostly useful for debugging purposes.
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
            self.assert_offset_of_leaf(cursor.leaf_idx, cursor.offset);
            self.assert_child_idx_of_leaf(cursor.leaf_idx, cursor.child_idx);
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

    /// TODO: docs
    #[inline(always)]
    pub fn append(&mut self, leaf: L) -> LeafIdx<L> {
        let (last_leaf_idx, idx_in_parent) = self.last_leaf();
        self.insert_leaf_after_leaf(last_leaf_idx, idx_in_parent, leaf)
    }

    /// Returns an `(empty_leaves, total_leaves)` tuple.
    ///
    /// This is mostly useful for debugging purposes.
    pub fn count_empty_leaves(&self) -> (usize, usize) {
        let empty_leaves = self
            .lnodes
            .iter()
            .map(Lnode::value)
            .filter(|leaf| leaf.is_empty())
            .count();

        (empty_leaves, self.lnodes.len())
    }

    /// TODO: docs
    pub fn debug_as_btree(&self) -> debug::DebugAsBtree<'_, ARITY, L> {
        self.debug_inode_as_btree(self.root_idx)
    }

    /// TODO: docs
    pub fn debug_as_self(&self) -> debug::DebugAsSelf<'_, ARITY, L> {
        debug::DebugAsSelf(self)
    }

    /// TODO: docs
    #[inline]
    pub fn delete<DelRange, DelFrom, DelUpTo>(
        &mut self,
        range: Range<L::Length>,
        delete_range: DelRange,
        delete_from: DelFrom,
        delete_up_to: DelUpTo,
    ) -> (Option<LeafIdx<L>>, Option<LeafIdx<L>>)
    where
        L: Delete,
        DelRange: FnOnce(&mut L, Range<L::Length>) -> (Option<L>, Option<L>),
        DelFrom: FnOnce(&mut L, L::Length) -> Option<L>,
        DelUpTo: FnOnce(&mut L, L::Length) -> Option<L>,
    {
        if let Some(cursor) = self.cursor {
            let cursor_end = cursor.offset + self.leaf(cursor.leaf_idx).len();

            if (cursor.offset..cursor_end).contains_range(range) {
                return self.delete_range_in_leaf(
                    cursor.leaf_idx,
                    cursor.offset,
                    cursor.child_idx,
                    range - cursor.offset,
                    delete_range,
                );
            }

            if let Some((leaf_idx, child_idx)) =
                self.next_non_empty_leaf(cursor.leaf_idx, cursor.child_idx)
            {
                let next_len = self.leaf(leaf_idx).len();

                if (cursor_end..cursor_end + next_len).contains_range(range) {
                    return self.delete_range_in_leaf(
                        leaf_idx,
                        cursor_end,
                        child_idx,
                        range - cursor_end,
                        delete_range,
                    );
                }
            }
        }

        self.delete_range(range, delete_range, delete_from, delete_up_to)
    }

    /// TODO: docs
    #[inline]
    pub fn get_leaf(&self, leaf_idx: LeafIdx<L>) -> &L {
        self.leaf(leaf_idx)
    }

    /// TODO: docs
    #[inline]
    pub fn get_leaf_mut<F>(&mut self, leaf_idx: LeafIdx<L>, with_leaf: F)
    where
        F: FnOnce(&mut L),
    {
        let lnode = self.lnode_mut(leaf_idx);

        let old_len = lnode.value().len();
        with_leaf(lnode.value_mut());
        let new_len = lnode.value().len();

        if old_len != new_len {
            let diff = L::Length::diff(old_len, new_len);
            let parent_idx = lnode.parent();
            self.apply_diff(parent_idx, diff);
        }
    }

    /// TODO: docs
    #[inline]
    pub fn get_last_leaf_mut<F>(&mut self, with_leaf: F)
    where
        F: FnOnce(&mut L),
    {
        let (last_idx, _) = self.last_leaf();
        self.get_leaf_mut(last_idx, with_leaf);
    }

    /// TODO: docs
    #[inline]
    pub fn get_next_leaf(&self, leaf_idx: LeafIdx<L>) -> LeafIdx<L> {
        let idx_in_parent = self.idx_of_leaf_in_parent(leaf_idx);

        let parent_idx = self.lnode(leaf_idx).parent();

        let parent = self.inode(parent_idx);

        if idx_in_parent + 1 < parent.num_children() {
            return parent.child(idx_in_parent + 1).unwrap_leaf();
        }

        let mut inode_idx = parent_idx;

        loop {
            debug_assert!(!self.is_root(inode_idx));

            let idx_in_parent = self.idx_of_inode_in_parent(inode_idx);
            let parent_idx = self.inode(inode_idx).parent();
            let parent = self.inode(parent_idx);

            if idx_in_parent + 1 < parent.num_children() {
                inode_idx = parent.child(idx_in_parent + 1).unwrap_inode();
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
                    return leaf_idxs[0];
                },
            }
        }
    }

    /// TODO: docs
    #[inline]
    pub fn get_prev_leaf(&self, leaf_idx: LeafIdx<L>) -> LeafIdx<L> {
        let idx_in_parent = self.idx_of_leaf_in_parent(leaf_idx);

        let parent_idx = self.lnode(leaf_idx).parent();

        let parent = self.inode(parent_idx);

        if idx_in_parent > 0 {
            return parent.child(idx_in_parent - 1).unwrap_leaf();
        }

        let mut inode_idx = parent_idx;

        loop {
            debug_assert!(!self.is_root(inode_idx));

            let idx_in_parent = self.idx_of_inode_in_parent(inode_idx);
            let parent_idx = self.inode(inode_idx).parent();
            let parent = self.inode(parent_idx);

            if idx_in_parent > 0 {
                inode_idx = parent.child(idx_in_parent - 1).unwrap_inode();
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
                    return leaf_idxs[leaf_idxs.len() - 1];
                },
            }
        }
    }

    /// TODO: docs
    #[inline]
    pub fn with_two_mut<F>(
        &mut self,
        first_idx: LeafIdx<L>,
        second_idx: LeafIdx<L>,
        with_two: F,
    ) where
        F: FnOnce(&mut L, &mut L),
    {
        debug_assert!(first_idx.idx < second_idx.idx);

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

        let first_diff = L::Length::diff(old_first_len, first.value().len());

        let second_diff =
            L::Length::diff(old_second_len, second.value().len());

        self.apply_diff(first_parent, first_diff);

        self.apply_diff(second_parent, second_diff);
    }

    /// TODO: docs
    #[inline]
    pub fn from_children<I>(children: I, tot_len: L::Length) -> Self
    where
        I: ExactSizeIterator<Item = L>,
    {
        let len = children.len();

        let root_idx = InodeIdx(0);

        let mut inode_children = [NodeIdx::dangling(); ARITY];

        let mut lnodes = Vec::with_capacity(children.len());

        for (i, child) in children.enumerate() {
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

    /// TODO: docs
    #[inline]
    pub fn initialize(&mut self, first_leaf: L) -> LeafIdx<L> {
        debug_assert!(self.inodes.is_empty());
        debug_assert!(self.lnodes.is_empty());
        let len = first_leaf.len();
        let leaf_idx = self.push_leaf(first_leaf, InodeIdx(0));
        let root = Inode::from_leaf(leaf_idx, len, InodeIdx::dangling());
        self.root_idx = self.push_inode(root, InodeIdx::dangling());
        leaf_idx
    }

    /// TODO: docs
    #[inline]
    pub fn insert<F>(
        &mut self,
        offset: L::Length,
        insert_with: F,
    ) -> (Option<LeafIdx<L>>, Option<LeafIdx<L>>)
    where
        F: FnOnce(&mut L, L::Length) -> (Option<L>, Option<L>),
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

    /// TODO: docs
    #[inline]
    pub fn is_initialized(&self) -> bool {
        !self.inodes.is_empty()
    }

    /// TODO: docs
    #[inline]
    pub fn leaf_at_offset(
        &self,
        offset: L::Length,
    ) -> (LeafIdx<L>, L::Length) {
        if let Some(cursor) = self.cursor {
            let cursor_end = cursor.offset + self.leaf(cursor.leaf_idx).len();

            if offset > cursor.offset && offset <= cursor_end {
                return (cursor.leaf_idx, cursor.offset);
            }
        }

        let mut leaf_offset = L::Length::zero();

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

    #[inline]
    pub fn leaves(&self) -> Leaves<'_, ARITY, L> {
        self.into()
    }

    #[inline(always)]
    pub fn len(&self) -> L::Length {
        self.root().len()
    }

    /// Creates a new Gtree with the given leaf as its first leaf.
    #[inline]
    pub fn new(first_leaf: L) -> (Self, LeafIdx<L>) {
        let mut this = Self::uninit();
        let idx = this.initialize(first_leaf);
        (this, idx)
    }

    /// TODO: docs
    #[inline]
    pub fn prepend(&mut self, leaf: L) -> LeafIdx<L> {
        let first_leaf_idx = {
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
        };

        let leaf_idx = self.insert_leaf_before_leaf(first_leaf_idx, 0, leaf);

        self.cursor = Some(Cursor::new(leaf_idx, L::Length::zero(), 0));

        leaf_idx
    }

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

        let old_len = lnode.value().len();
        let split_leaf = split_with(lnode.value_mut());
        let new_len = lnode.value().len();

        if old_len != new_len {
            let diff = L::Length::diff(old_len, new_len);
            self.apply_diff(parent_idx, diff);
        }

        let idx_in_parent = self.inode(parent_idx).idx_of_leaf_child(leaf_idx);
        self.insert_leaf_after_leaf(leaf_idx, idx_in_parent, split_leaf)
    }

    #[inline]
    pub fn uninit() -> Self {
        Self {
            inodes: Vec::new(),
            lnodes: Vec::new(),
            root_idx: InodeIdx::dangling(),
            cursor: None,
        }
    }
}

// Private API.
impl<const ARITY: usize, L: Leaf> Gtree<ARITY, L> {
    /// Applies the diff to the len of every internal node starting
    /// from the given inode and going up the tree until the root.
    #[inline]
    fn apply_diff(
        &mut self,
        starting_from: InodeIdx,
        diff: <L::Length as Length>::Diff,
    ) {
        let mut idx = starting_from;

        while !idx.is_dangling() {
            let inode = self.inode_mut(idx);
            inode.len_mut().apply_diff(diff);
            idx = inode.parent();
        }
    }

    /// TODO: docs
    fn assert_child_idx_of_leaf(
        &self,
        leaf_idx: LeafIdx<L>,
        child_idx: ChildIdx,
    ) {
        let parent = self.inode(self.lnode(leaf_idx).parent());
        assert_eq!(child_idx, parent.idx_of_leaf_child(leaf_idx));
    }

    /// TODO: docs
    fn assert_inode_invariants(&self, inode_idx: InodeIdx) {
        let inode = self.inode(inode_idx);

        let inode_height = self.inode_height(inode_idx);

        let mut child_lengths = L::Length::zero();

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

        if !inode.len().is_zero() {
            assert_eq!(child_lengths, inode.len());
        }
    }

    /// Asserts that the len offset of the given leaf is equal to the
    /// given len.
    ///
    /// The len offset of a leaf is the sum of the lengths of all the
    /// leaves that precede it in the Gtree, *not* including the len of
    /// the leaf itself.
    fn assert_offset_of_leaf(
        &self,
        leaf_idx: LeafIdx<L>,
        leaf_offset: L::Length,
    ) {
        let mut len = L::Length::zero();

        let mut parent_idx = self.lnode(leaf_idx).parent();

        let parent = self.inode(parent_idx);
        let child_idx = parent.idx_of_leaf_child(leaf_idx);
        len += self.len_offset_of_child(parent_idx, child_idx);
        let mut inode_idx = parent_idx;
        parent_idx = parent.parent();

        while !parent_idx.is_dangling() {
            let parent = self.inode(parent_idx);
            let child_idx = parent.idx_of_internal_child(inode_idx);
            len += self.len_offset_of_child(parent_idx, child_idx);
            inode_idx = parent_idx;
            parent_idx = parent.parent();
        }

        assert_eq!(len, leaf_offset);
    }

    /// TODO: docs
    #[inline]
    fn bubble(
        &mut self,
        mut inode_idx: InodeIdx,
        mut maybe_split: Option<Inode<ARITY, L>>,
        mut patch: <L::Length as Length>::Diff,
        leaf_patch: <L::Length as Length>::Diff,
    ) {
        let mut parent_idx = self.inode(inode_idx).parent();

        loop {
            if parent_idx.is_dangling() {
                if let Some(split) = maybe_split {
                    self.root_has_split(split);
                }
                break;
            } else if let Some(split) = maybe_split {
                let split_len = split.len();
                let split_idx = self.push_inode(split, parent_idx);

                let parent = self.inode_mut(parent_idx);
                let old_len = parent.len();
                parent.len_mut().apply_diff(patch);
                let idx_in_parent = parent.idx_of_internal_child(inode_idx);

                maybe_split = self.insert_in_inode(
                    parent_idx,
                    idx_in_parent + 1,
                    NodeIdx::from_internal(split_idx),
                    split_len,
                );

                let parent = self.inode(parent_idx);
                patch = L::Length::diff(old_len, parent.len());
                inode_idx = parent_idx;
                parent_idx = parent.parent();
            } else {
                self.apply_diff(parent_idx, leaf_patch);
                break;
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
        at_offset: L::Length,
    ) -> (ChildIdx, L::Length) {
        debug_assert!(at_offset <= self.inode(of_inode).len());

        let mut offset = L::Length::zero();

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
        range: Range<L::Length>,
    ) -> Option<(ChildIdx, L::Length)> {
        debug_assert!(range.start <= range.end);
        debug_assert!(range.end <= self.inode(of_inode).len());

        #[inline(always)]
        fn measure<L, I>(
            iter: I,
            range: Range<L::Length>,
        ) -> Option<(ChildIdx, L::Length)>
        where
            L: Leaf,
            I: Iterator<Item = L::Length>,
        {
            let mut offset = L::Length::zero();
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
    ) -> L::Length {
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
    fn delete_child(&mut self, of_inode: InodeIdx, child_idx: ChildIdx)
    where
        L: Delete,
    {
        let child_len = match self.inode(of_inode).child(child_idx) {
            Either::Internal(inode_idx) => {
                let inode = self.inode_mut(inode_idx);
                let old_len = inode.len();
                self.delete_inode(inode_idx);
                old_len
            },
            Either::Leaf(leaf_idx) => {
                let leaf = self.leaf_mut(leaf_idx);
                let old_len = leaf.len();
                leaf.delete();
                old_len
            },
        };

        *self.inode_mut(of_inode).len_mut() -= child_len;
    }

    #[inline]
    fn delete_inode(&mut self, inode: InodeIdx)
    where
        L: Delete,
    {
        let inode = self.inode_mut(inode);

        *inode.len_mut() = L::Length::zero();

        // In both arms of the match we transmute the slices back into the same
        // types to iterate over the inode/leaf indices and delete the
        // inodes/leaves at the same time, which Rust's aliasing rules would
        // prohibit.
        //
        // SAFETY: left as an exercise to the reader (it's safe).

        match inode.children() {
            Either::Internal(inode_idxs) => {
                let inode_idxs =
                    unsafe { mem::transmute::<_, &[InodeIdx]>(inode_idxs) };
                for &inode_idx in inode_idxs {
                    self.delete_inode(inode_idx);
                }
            },

            Either::Leaf(leaf_idxs) => {
                let leaf_idxs =
                    unsafe { mem::transmute::<_, &[LeafIdx<L>]>(leaf_idxs) };
                for &leaf_idx in leaf_idxs {
                    self.leaf_mut(leaf_idx).delete();
                }
            },
        }
    }

    /// TODO: docs
    #[inline]
    fn delete_range_in_leaf<F>(
        &mut self,
        leaf_idx: LeafIdx<L>,
        leaf_offset: L::Length,
        idx_in_parent: ChildIdx,
        range: Range<L::Length>,
        delete_with: F,
    ) -> (Option<LeafIdx<L>>, Option<LeafIdx<L>>)
    where
        L: Delete,
        F: FnOnce(&mut L, Range<L::Length>) -> (Option<L>, Option<L>),
    {
        debug_assert!(range.start < range.end);
        debug_assert!(range.end - range.start <= self.leaf(leaf_idx).len());
        debug_assert_eq!(idx_in_parent, self.idx_of_leaf_in_parent(leaf_idx));

        let lnode = self.lnode_mut(leaf_idx);

        let old_len = lnode.value().len();

        let (first, second) = delete_with(lnode.value_mut(), range);

        let patch = L::Length::diff(old_len, lnode.value().len());

        let parent_idx = lnode.parent();

        self.apply_diff(parent_idx, patch);

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
                    match self.next_leaf(leaf_idx, idx_in_parent) {
                        Some(next_idx) => self
                            .leaf_mut(next_idx)
                            .prepend(deleted)
                            .map(|deleted| {
                                self.insert_leaf_after_leaf(
                                    leaf_idx,
                                    idx_in_parent,
                                    deleted,
                                )
                            }),

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
                    .previous_leaf(leaf_idx, idx_in_parent)
                {
                    Some(prev_idx) => {
                        let patch =
                            L::Length::diff(L::Length::zero(), rest.len());

                        let deleted =
                            core::mem::replace(self.leaf_mut(leaf_idx), rest);

                        let previous_leaf = self.leaf_mut(prev_idx);

                        if let Some(deleted) = previous_leaf.append(deleted) {
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
                            self.apply_diff(parent_idx, patch);
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
                    .previous_non_empty_leaf(leaf_idx, idx_in_parent)
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
                    .previous_non_empty_leaf(leaf_idx, idx_in_parent)
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
    fn delete_range<DelRange, DelFrom, DelUpTo>(
        &mut self,
        mut range: Range<L::Length>,
        delete_range: DelRange,
        delete_from: DelFrom,
        delete_up_to: DelUpTo,
    ) -> (Option<LeafIdx<L>>, Option<LeafIdx<L>>)
    where
        L: Delete,
        DelRange: FnOnce(&mut L, Range<L::Length>) -> (Option<L>, Option<L>),
        DelFrom: FnOnce(&mut L, L::Length) -> Option<L>,
        DelUpTo: FnOnce(&mut L, L::Length) -> Option<L>,
    {
        // First, descend to the deepest node that fully contains the range.

        let mut idx = self.root_idx;

        let mut leaf_offset = L::Length::zero();

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
                        delete_from,
                        delete_up_to,
                    );
                },
            }
        }
    }

    #[inline]
    fn delete_range_in_inode<DelFrom, DelUpTo>(
        &mut self,
        idx: InodeIdx,
        range: Range<L::Length>,
        delete_from: DelFrom,
        delete_up_to: DelUpTo,
    ) -> (Option<LeafIdx<L>>, Option<LeafIdx<L>>)
    where
        L: Delete,
        DelFrom: FnOnce(&mut L, L::Length) -> Option<L>,
        DelUpTo: FnOnce(&mut L, L::Length) -> Option<L>,
    {
        debug_assert!(self.child_containing_range(idx, range).is_none());

        self.cursor = None;

        let old_len = self.inode(idx).len();

        let ((first, second), maybe_split) = delete::delete_range_in_inode(
            self,
            idx,
            range,
            delete_from,
            delete_up_to,
        );

        let patch = L::Length::diff(old_len, self.inode(idx).len());

        let leaf_patch = L::Length::diff(
            old_len,
            self.inode(idx).len()
                + maybe_split
                    .as_ref()
                    .map(Inode::len)
                    .unwrap_or(L::Length::zero()),
        );

        self.bubble(idx, maybe_split, patch, leaf_patch);

        (first, second)
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

    /// TODO: docs
    #[inline]
    fn insert_at_leaf<F>(
        &mut self,
        leaf_idx: LeafIdx<L>,
        leaf_offset: L::Length,
        idx_in_parent: ChildIdx,
        insert_at_offset: L::Length,
        insert_with: F,
    ) -> (Option<LeafIdx<L>>, Option<LeafIdx<L>>)
    where
        F: FnOnce(&mut L, L::Length) -> (Option<L>, Option<L>),
    {
        debug_assert!(insert_at_offset > L::Length::zero());

        let lnode = self.lnode_mut(leaf_idx);

        let old_len = lnode.value().len();

        let (first, second) = insert_with(lnode.value_mut(), insert_at_offset);

        let new_len = lnode.value().len();

        let patch = L::Length::diff(old_len, new_len);

        let parent_idx = lnode.parent();

        self.apply_diff(parent_idx, patch);

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

    /// TODO: docs
    #[inline]
    fn insert_at_offset<F>(
        &mut self,
        offset: L::Length,
        insert_with: F,
    ) -> (Option<LeafIdx<L>>, Option<LeafIdx<L>>)
    where
        F: FnOnce(&mut L, L::Length) -> (Option<L>, Option<L>),
    {
        let (idxs, maybe_split) = insert::insert_at_offset(
            self,
            self.root_idx,
            L::Length::zero(),
            offset,
            insert_with,
        );

        if let Some(root_split) = maybe_split {
            self.root_has_split(root_split);
        }

        idxs
    }

    /// TODO: docs
    #[inline]
    fn insert_in_inode(
        &mut self,
        idx: InodeIdx,
        at_offset: usize,
        child: NodeIdx<L>,
        child_len: L::Length,
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

    /// TODO: docs
    #[inline]
    fn insert_leaf_after_leaf(
        &mut self,
        leaf_idx: LeafIdx<L>,
        idx_in_parent: ChildIdx,
        leaf: L,
    ) -> LeafIdx<L> {
        let leaf_len = leaf.len();

        let leaf_patch = L::Length::diff(L::Length::zero(), leaf_len);

        let parent_idx = self.lnode(leaf_idx).parent();

        let inserted_idx = self.push_leaf(leaf, parent_idx);

        let parent = self.inode(parent_idx);
        let old_len = parent.len();
        let idx_in_parent = idx_in_parent;

        let maybe_split = self.insert_in_inode(
            parent_idx,
            idx_in_parent + 1,
            NodeIdx::from_leaf(inserted_idx),
            leaf_len,
        );

        let parent = self.inode(parent_idx);
        let patch = L::Length::diff(old_len, parent.len());

        self.bubble(parent_idx, maybe_split, patch, leaf_patch);

        inserted_idx
    }

    /// TODO: docs
    #[inline]
    fn insert_leaf_before_leaf(
        &mut self,
        leaf_idx: LeafIdx<L>,
        idx_in_parent: ChildIdx,
        leaf: L,
    ) -> LeafIdx<L> {
        let leaf_len = leaf.len();

        let leaf_patch = L::Length::diff(L::Length::zero(), leaf_len);

        let parent_idx = self.lnode(leaf_idx).parent();

        let inserted_idx = self.push_leaf(leaf, parent_idx);

        let parent = self.inode(parent_idx);
        let old_len = parent.len();
        let idx_in_parent = idx_in_parent;

        let maybe_split = self.insert_in_inode(
            parent_idx,
            idx_in_parent,
            NodeIdx::from_leaf(inserted_idx),
            leaf_len,
        );

        let parent = self.inode(parent_idx);
        let patch = L::Length::diff(old_len, parent.len());

        self.bubble(parent_idx, maybe_split, patch, leaf_patch);

        inserted_idx
    }

    /// TODO: docs
    #[allow(clippy::too_many_arguments)]
    #[inline]
    fn insert_two_in_inode(
        &mut self,
        idx: InodeIdx,
        mut first_offset: usize,
        mut first_child: NodeIdx<L>,
        mut first_len: L::Length,
        mut second_offset: usize,
        mut second_child: NodeIdx<L>,
        mut second_len: L::Length,
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

    /// TODO: docs
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

        let leaf_patch =
            L::Length::diff(L::Length::zero(), first_len + second_len);

        let parent_idx = self.lnode(leaf_idx).parent();

        let first_idx = self.push_leaf(first_leaf, parent_idx);
        let second_idx = self.push_leaf(second_leaf, parent_idx);

        let parent = self.inode(parent_idx);
        let old_len = parent.len();
        let idx_in_parent = idx_in_parent;

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
        let patch = L::Length::diff(old_len, parent.len());

        self.bubble(parent_idx, maybe_split, patch, leaf_patch);

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
    fn leaf(&self, idx: LeafIdx<L>) -> &L {
        self.lnode(idx).value()
    }

    #[inline(always)]
    pub fn leaf_mut(&mut self, idx: LeafIdx<L>) -> &mut L {
        self.lnode_mut(idx).value_mut()
    }

    #[inline]
    fn lnode(&self, idx: LeafIdx<L>) -> &Lnode<L> {
        &self.lnodes[idx.idx]
    }

    #[inline]
    fn lnode_mut(&mut self, idx: LeafIdx<L>) -> &mut Lnode<L> {
        &mut self.lnodes[idx.idx]
    }

    /// TODO: docs
    #[inline]
    fn next_leaf(
        &self,
        leaf_idx: LeafIdx<L>,
        idx_in_parent: ChildIdx,
    ) -> Option<LeafIdx<L>> {
        let parent = self.inode(self.lnode(leaf_idx).parent());

        if idx_in_parent + 1 == parent.num_children() {
            None
        } else if let Either::Leaf(next) = parent.child(idx_in_parent + 1) {
            Some(next)
        } else {
            unreachable!()
        }
    }

    /// TODO: docs
    #[inline]
    fn next_non_empty_leaf(
        &self,
        leaf_idx: LeafIdx<L>,
        idx_in_parent: ChildIdx,
    ) -> Option<(LeafIdx<L>, ChildIdx)> {
        let parent = self.inode(self.lnode(leaf_idx).parent());

        let Either::Leaf(leaf_idxs) = parent.children() else {
            unreachable!()
        };

        leaf_idxs[idx_in_parent + 1..].iter().enumerate().find_map(
            |(idx, &leaf_idx)| {
                (!self.leaf(leaf_idx).is_empty())
                    .then_some((leaf_idx, idx_in_parent + 1 + idx))
            },
        )
    }

    /// TODO: docs
    #[inline]
    fn previous_leaf(
        &self,
        leaf_idx: LeafIdx<L>,
        idx_in_parent: ChildIdx,
    ) -> Option<LeafIdx<L>> {
        let parent = self.inode(self.lnode(leaf_idx).parent());

        if idx_in_parent == 0 {
            None
        } else if let Either::Leaf(next) = parent.child(idx_in_parent - 1) {
            Some(next)
        } else {
            unreachable!()
        }
    }

    /// TODO: docs
    #[inline]
    fn previous_non_empty_leaf(
        &self,
        leaf_idx: LeafIdx<L>,
        idx_in_parent: ChildIdx,
    ) -> Option<(LeafIdx<L>, ChildIdx)> {
        let parent = self.inode(self.lnode(leaf_idx).parent());

        let Either::Leaf(leaf_idxs) = parent.children() else {
            unreachable!()
        };

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

    #[inline]
    fn len_offset_of_child(
        &self,
        in_inode: InodeIdx,
        child_offset: ChildIdx,
    ) -> L::Length {
        let mut len = L::Length::zero();

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
    fn with_internal_mut<F, R>(&mut self, inode_idx: InodeIdx, fun: F) -> R
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
        let (ret, maybe_split) = self.with_internal_mut(inode_idx, fun);

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
    fn with_leaf_mut<F, T>(&mut self, leaf_idx: LeafIdx<L>, fun: F) -> T
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

        let (left, right) = self.with_leaf_mut(leaf_idx, fun);

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

/// The index inside the `children` array of a particular child of an internal
/// node.
type ChildIdx = usize;

/// An internal node of the Gtree.
#[derive(Clone)]
struct Inode<const ARITY: usize, L: Leaf> {
    /// The total len of this node, which is the sum of the lengths of
    /// all of its children.
    tot_len: L::Length,

    /// The index of this node's parent, or `InodeIdx::dangling()` if this is
    /// the root node.
    parent: InodeIdx,

    /// The number of children this inode is storing, which is always less than
    /// or equal to `ARITY`.
    num_children: usize,

    /// The indexes of this node's children in the Gtree. The first `len` of
    /// these are valid, and the rest are dangling.
    children: [NodeIdx<L>; ARITY],

    /// Whether `children` contains `LeafIdx<L>`s or `InodeIdx`s.
    has_leaves: bool,
}

/// An index to either an internal node or a leaf node of the Gtree.
///
/// We use a union here to save space, since we know that an inode can only
/// store either internal nodes or leaf nodes, but not both.
///
/// This means that we can use a single boolean (the `has_leaves` field of
/// `Inode`) instead of storing the same tag for every single child of the
/// inode, like we would have to do if we used an enum.
union NodeIdx<L: Leaf> {
    internal: InodeIdx,
    leaf: LeafIdx<L>,
}

impl<L: Leaf> Copy for NodeIdx<L> {}

impl<L: Leaf> Clone for NodeIdx<L> {
    #[inline]
    fn clone(&self) -> Self {
        *self
    }
}

impl<L: Leaf> NodeIdx<L> {
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

enum Either<I, L> {
    Internal(I),
    Leaf(L),
}

impl<I, L> Either<I, L> {
    #[inline]
    fn unwrap_inode(self) -> I {
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
    fn from_leaf(leaf: LeafIdx<L>, len: L::Length, parent: InodeIdx) -> Self {
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
        first_len: L::Length,
        second_len: L::Length,
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
        let Either::Internal(idxs) = self.children() else {
            panic!("this inode contains leaf nodes");
        };

        idxs.iter()
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
        let Either::Leaf(idxs) = self.children() else {
                panic!("this inode contains other inodes");
            };

        idxs.iter()
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
        first_len: L::Length,
        second_offset: usize,
        second_child: NodeIdx<L>,
        second_len: L::Length,
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
        child_len: L::Length,
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
    fn num_children(&self) -> usize {
        self.num_children
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
    fn split(&mut self, at_offset: usize, new_len: L::Length) -> Self {
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

    #[inline(always)]
    fn len(&self) -> L::Length {
        self.tot_len
    }

    #[inline(always)]
    fn len_mut(&mut self) -> &mut L::Length {
        &mut self.tot_len
    }
}

/// A leaf node of the Gtree.
#[derive(Debug, Clone)]
struct Lnode<Leaf> {
    /// The value of this leaf node.
    value: Leaf,

    /// The index of this leaf node's parent.
    parent: InodeIdx,
}

impl<L: Leaf> Lnode<L> {
    #[inline(always)]
    fn new(value: L, parent: InodeIdx) -> Self {
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
    //! TODO: docs.

    use super::*;

    #[allow(clippy::type_complexity)]
    #[inline]
    pub(super) fn insert_at_offset<const N: usize, L, F>(
        gtree: &mut Gtree<N, L>,
        in_inode: InodeIdx,
        mut leaf_offset: L::Length,
        at_offset: L::Length,
        insert_with: F,
    ) -> ((Option<LeafIdx<L>>, Option<LeafIdx<L>>), Option<Inode<N, L>>)
    where
        L: Leaf,
        F: FnOnce(&mut L, L::Length) -> (Option<L>, Option<L>),
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

                gtree.cursor = if at_offset == L::Length::zero() {
                    // TODO: why?
                    None
                } else if let Some(idx) = inserted_idx {
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
    //! TODO: docs.

    use super::*;

    #[allow(clippy::type_complexity)]
    pub(super) fn delete_range_in_inode<const N: usize, L, DelFrom, DelUpTo>(
        gtree: &mut Gtree<N, L>,
        idx: InodeIdx,
        range: Range<L::Length>,
        del_from: DelFrom,
        del_up_to: DelUpTo,
    ) -> ((Option<LeafIdx<L>>, Option<LeafIdx<L>>), Option<Inode<N, L>>)
    where
        L: Leaf + Delete,
        DelFrom: FnOnce(&mut L, L::Length) -> Option<L>,
        DelUpTo: FnOnce(&mut L, L::Length) -> Option<L>,
    {
        let mut idx_start = 0;
        let mut leaf_idx_start = None;
        let mut extra_from_start = None;

        let mut idx_end = 0;
        let mut leaf_idx_end = None;
        let mut extra_from_end = None;

        let mut idxs = 0..gtree.inode(idx).num_children();
        let mut offset = L::Length::zero();

        for child_idx in idxs.by_ref() {
            let child_measure = gtree.child_measure(idx, child_idx);

            offset += child_measure;

            if offset > range.start {
                offset -= child_measure;

                idx_start = child_idx;

                match gtree.inode(idx).child(child_idx) {
                    Either::Internal(inode_idx) => {
                        let (leaf_idx, split) =
                            gtree.with_internal_mut(inode_idx, |gtree| {
                                delete_from(
                                    gtree,
                                    inode_idx,
                                    range.start - offset,
                                    del_from,
                                )
                            });

                        leaf_idx_start = leaf_idx;

                        if let Some(extra) = split {
                            let len = extra.len();
                            let inode_idx = gtree.push_inode(extra, idx);
                            let node_idx = NodeIdx::from_internal(inode_idx);
                            extra_from_start = Some((node_idx, len));
                        }
                    },

                    Either::Leaf(leaf_idx) => {
                        let split = gtree.with_leaf_mut(leaf_idx, |leaf| {
                            del_from(leaf, range.start - offset)
                        });

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
                        let (leaf_idx, split) =
                            gtree.with_internal_mut(inode_idx, |gtree| {
                                delete_up_to(
                                    gtree,
                                    inode_idx,
                                    range.end - offset,
                                    del_up_to,
                                )
                            });

                        leaf_idx_end = leaf_idx;

                        if let Some(extra) = split {
                            let len = extra.len();
                            let inode_idx = gtree.push_inode(extra, idx);
                            let node_idx = NodeIdx::from_internal(inode_idx);
                            extra_from_end = Some((node_idx, len));
                        }
                    },

                    Either::Leaf(leaf_idx) => {
                        let split = gtree.with_leaf_mut(leaf_idx, |leaf| {
                            del_up_to(leaf, range.end - offset)
                        });

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
                gtree.delete_child(idx, child_idx);
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

    fn delete_from<const N: usize, L, DelFrom>(
        gtree: &mut Gtree<N, L>,
        inode_idx: InodeIdx,
        mut from: L::Length,
        del_from: DelFrom,
    ) -> (Option<LeafIdx<L>>, Option<Inode<N, L>>)
    where
        L: Leaf + Delete,
        DelFrom: FnOnce(&mut L, L::Length) -> Option<L>,
    {
        let len = gtree.inode(inode_idx).num_children();

        let mut offset = L::Length::zero();

        for child_idx in 0..len {
            let child_measure = gtree.child_measure(inode_idx, child_idx);

            offset += child_measure;

            if offset > from {
                for child_idx in child_idx + 1..len {
                    gtree.delete_child(inode_idx, child_idx);
                }

                offset -= child_measure;

                from -= offset;

                return match gtree.inode(inode_idx).child(child_idx) {
                    Either::Internal(next_idx) => gtree
                        .with_internal_mut_handle_split(
                            next_idx,
                            child_idx,
                            |gtree| {
                                delete_from(gtree, next_idx, from, del_from)
                            },
                        ),

                    Either::Leaf(leaf_idx) => {
                        let (idx, _none, split) = gtree
                            .with_leaf_mut_handle_split(
                                leaf_idx,
                                child_idx,
                                |leaf| (del_from(leaf, from), None),
                            );

                        (idx, split)
                    },
                };
            }
        }

        unreachable!();
    }

    fn delete_up_to<const N: usize, L, DelUpTo>(
        gtree: &mut Gtree<N, L>,
        idx: InodeIdx,
        mut up_to: L::Length,
        del_up_to: DelUpTo,
    ) -> (Option<LeafIdx<L>>, Option<Inode<N, L>>)
    where
        L: Leaf + Delete,
        DelUpTo: FnOnce(&mut L, L::Length) -> Option<L>,
    {
        let mut offset = L::Length::zero();

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
                                delete_up_to(gtree, next_idx, up_to, del_up_to)
                            },
                        ),

                    Either::Leaf(leaf_idx) => {
                        let (idx, _none, split) = gtree
                            .with_leaf_mut_handle_split(
                                leaf_idx,
                                child_idx,
                                |leaf| (del_up_to(leaf, up_to), None),
                            );

                        (idx, split)
                    },
                };
            } else {
                gtree.delete_child(idx, child_idx);
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

            if gtree.is_initialized() {
                f.debug_map()
                    .entry(&inodes, &debug_inodes)
                    .entry(&lnodes, &debug_lnodes)
                    .finish()
            } else {
                let empty_slice: &[()] = &[];

                f.debug_map()
                    .entry(&DebugAsDisplay(&"inodes"), &empty_slice)
                    .entry(&DebugAsDisplay(&"lnodes"), &empty_slice)
                    .finish()
            }
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

            if self.gtree.is_initialized() {
                writeln!(f)?;

                print_inode_as_tree(
                    self.gtree,
                    self.inode_idx,
                    &mut String::new(),
                    "",
                    0,
                    f,
                )
            } else {
                f.debug_map().finish()
            }
        }
    }
}

pub use leaves::Leaves;

mod leaves {
    use super::*;

    #[derive(Debug)]
    pub struct Leaves<'a, const N: usize, L: Leaf> {
        gtree: &'a Gtree<N, L>,
        path: Vec<(InodeIdx, ChildIdx)>,
        current_leaves: &'a [LeafIdx<L>],
    }

    impl<'a, const N: usize, L: Leaf> From<&'a Gtree<N, L>> for Leaves<'a, N, L> {
        fn from(gtree: &'a Gtree<N, L>) -> Self {
            let mut path = Vec::new();
            let mut idx = gtree.root_idx;
            let mut current_leaves = &[][..];

            if gtree.is_initialized() {
                loop {
                    match gtree.inode(idx).children() {
                        Either::Internal(inode_idxs) => {
                            path.push((idx, 0));
                            idx = inode_idxs[0];
                        },

                        Either::Leaf(leaf_idxs) => {
                            current_leaves = leaf_idxs;
                            break;
                        },
                    }
                }
            }

            Self { gtree, path, current_leaves }
        }
    }

    impl<'a, const N: usize, L: Leaf> Iterator for Leaves<'a, N, L> {
        type Item = &'a L;

        fn next(&mut self) -> Option<Self::Item> {
            if !self.gtree.is_initialized() {
                return None;
            }

            if let Some((first, rest)) = self.current_leaves.split_first() {
                let leaf = self.gtree.leaf(*first);
                self.current_leaves = rest;
                Some(leaf)
            } else {
                while let Some((idx, child_idx)) = self.path.last_mut() {
                    *child_idx += 1;
                    if *child_idx < self.gtree.inode(*idx).num_children() {
                        break;
                    }
                    self.path.pop();
                }

                let &(last_idx, child_idx) = self.path.last()?;

                let mut idx =
                    self.gtree.inode(last_idx).child(child_idx).unwrap_inode();

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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug)]
    struct TestLeaf {
        len: u64,
        is_visible: bool,
    }

    impl Delete for TestLeaf {
        fn delete(&mut self) {
            self.is_visible = false;
        }
    }

    impl Join for TestLeaf {
        fn append(&mut self, other: Self) -> Option<Self> {
            Some(other)
        }
        fn prepend(&mut self, other: Self) -> Option<Self> {
            Some(other)
        }
    }

    impl Leaf for TestLeaf {
        type Length = u64;

        fn len(&self) -> Self::Length {
            self.len * self.is_visible as u64
        }
    }

    impl TestLeaf {
        fn insert_with_len(
            len: u64,
        ) -> impl FnOnce(&mut Self, u64) -> (Option<Self>, Option<Self>)
        {
            move |leaf, offset| {
                debug_assert!(offset <= leaf.len);

                if offset == 0 {
                    let new_leaf = TestLeaf::new_with_len(len);
                    (Some(mem::replace(leaf, new_leaf)), None)
                } else if offset == leaf.len {
                    (Some(TestLeaf::new_with_len(len)), None)
                } else {
                    let rest = leaf.split(offset);
                    (Some(TestLeaf::new_with_len(len)), rest)
                }
            }
        }

        fn new_with_len(len: u64) -> Self {
            Self { len, is_visible: true }
        }

        fn split(&mut self, at_offset: u64) -> Option<Self> {
            if at_offset >= self.len {
                None
            } else {
                let split = Self {
                    len: self.len - at_offset,
                    is_visible: self.is_visible,
                };
                self.len = at_offset;
                Some(split)
            }
        }
    }

    type TestGtree<const ARITY: usize> = Gtree<ARITY, TestLeaf>;

    #[test]
    fn insert_two_leaves_after_leaf_0() {
        let first_leaf = TestLeaf::new_with_len(1);

        let (mut gt, _) = TestGtree::<4>::new(first_leaf);

        let (Some(second_leaf_idx), _) =
            gt.insert(1, TestLeaf::insert_with_len(2)) else { panic!() };

        let third_leaf = TestLeaf::new_with_len(3);

        let fourth_leaf = TestLeaf::new_with_len(4);

        let _ = gt.insert_two_leaves_after_leaf(
            second_leaf_idx,
            0,
            third_leaf,
            fourth_leaf,
        );

        assert_eq!(gt.len(), 1 + 2 + 3 + 4);

        assert_eq!(gt.height(), 1);

        gt.assert_invariants();
    }

    #[test]
    fn insert_two_leaves_after_leaf_1() {
        let first_leaf = TestLeaf::new_with_len(2);

        let (mut gt, _) = TestGtree::<4>::new(first_leaf);

        let (Some(second_leaf_idx), _) =
            gt.insert(1, TestLeaf::insert_with_len(3)) else { panic!() };

        let third_leaf = TestLeaf::new_with_len(4);

        let fourth_leaf = TestLeaf::new_with_len(5);

        let _ = gt.insert_two_leaves_after_leaf(
            second_leaf_idx,
            0,
            third_leaf,
            fourth_leaf,
        );

        assert_eq!(gt.len(), 2 + 3 + 4 + 5);

        assert_eq!(gt.height(), 2);

        gt.assert_invariants();
    }
}
