use core::fmt::Debug;
use core::marker::PhantomData;
use core::mem;
use core::ops::{Add, AddAssign, Sub, SubAssign};

use crate::{Range, RangeExt};

/// A trait used to summarize the contents of a leaf node.
///
/// The `Add{Assign}` bounds are use to group together multiple summaries into
/// a single summary, allowing to "bubble up" node summaries up to the root.
pub trait Summary:
    Debug
    + Copy
    + Add<Self, Output = Self>
    + AddAssign<Self>
    + Sub<Self, Output = Self>
    + SubAssign<Self>
    + PartialEq<Self>
{
    /// This type is used to represent the difference between two summaries.
    type Diff: Debug + Copy;

    /// Creates a diff which, when applied to `old_summary`, produces
    /// `new_summary`.
    fn diff(old_summary: Self, new_summary: Self) -> Self::Diff;

    /// Applies a diff to `self`.
    fn apply_diff(&mut self, patch: Self::Diff);

    /// Returns a summary which represents the empty state.
    fn empty() -> Self;

    /// Returns `true` if this summary represents the empty state.
    fn is_empty(&self) -> bool {
        *self == Self::empty()
    }
}

/// A type which can be summarized. See [`Summary`] for more information.
pub trait Summarize {
    type Summary: Summary;

    /// Returns a summary of `self`.
    fn summarize(&self) -> Self::Summary;
}

/// A trait used to compute the "length" of a summary.
pub trait Length<S: Summary>:
    Debug
    + Copy
    + Add<Self, Output = Self>
    + AddAssign<Self>
    + Sub<Self, Output = Self>
    + SubAssign<Self>
    + Ord
{
    /// Returns the length of the empty summary.
    fn zero() -> Self;

    /// Returns the length of the given summary.
    fn len(summary: &S) -> Self;
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
pub trait Leaf: Debug + Summarize + Join {
    type Length: Length<Self::Summary>;

    #[inline]
    fn is_empty(&self) -> bool {
        Self::Length::len(&self.summarize()) == Self::Length::zero()
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
        let total = self.inodes.iter().map(Inode::len).sum::<usize>();
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
            let cursor_end = cursor.offset + self.leaf_len(cursor.leaf_idx);

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
                let next_len = self.leaf_len(leaf_idx);

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

        let old_summary = lnode.value().summarize();
        with_leaf(lnode.value_mut());
        let new_summary = lnode.value().summarize();

        if old_summary != new_summary {
            let diff = L::Summary::diff(old_summary, new_summary);
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
        todo!();
    }

    /// TODO: docs
    #[inline]
    pub fn get_prev_leaf(&self, leaf_idx: LeafIdx<L>) -> LeafIdx<L> {
        todo!();
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

        let split_at = first_idx.idx + 1;
        let (first, second) = self.lnodes.split_at_mut(split_at);
        let first = first[first_idx.idx].value_mut();
        let second = second[second_idx.idx - split_at].value_mut();
        with_two(first, second)
    }

    /// TODO: docs
    #[inline]
    pub fn from_children<I>(children: I, tot_summary: L::Summary) -> Self
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
            summary: tot_summary,
            parent: InodeIdx::dangling(),
            len,
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
        let summary = first_leaf.summarize();
        let leaf_idx = self.push_leaf(first_leaf, InodeIdx(0));
        let root = Inode::from_leaf(leaf_idx, summary, InodeIdx::dangling());
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
            let cursor_end = cursor.offset + self.leaf_len(cursor.leaf_idx);

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
            let cursor_end = cursor.offset + self.leaf_len(cursor.leaf_idx);

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
        self.inode_len(self.root_idx)
    }

    /// Creates a new Gtree with the given leaf as its first leaf.
    #[inline]
    pub fn new(first_leaf: L) -> (Self, LeafIdx<L>) {
        let mut this = Self::uninit();
        let idx = this.initialize(first_leaf);
        (this, idx)
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

        let old_summary = lnode.value().summarize();
        let split_leaf = split_with(lnode.value_mut());
        let new_summary = lnode.value().summarize();

        if old_summary != new_summary {
            let diff = L::Summary::diff(old_summary, new_summary);
            self.apply_diff(parent_idx, diff);
        }

        let idx_in_parent = self.inode(parent_idx).idx_of_leaf_child(leaf_idx);
        self.insert_leaf_after_leaf(leaf_idx, idx_in_parent, split_leaf)
    }

    #[inline]
    pub fn summary(&self) -> L::Summary {
        self.root().summary()
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
    /// Applies the diff to the summary of every internal node starting
    /// from the given inode and going up the tree until the root.
    #[inline]
    fn apply_diff(
        &mut self,
        starting_from: InodeIdx,
        diff: <L::Summary as Summary>::Diff,
    ) {
        let mut idx = starting_from;

        while !idx.is_dangling() {
            let inode = self.inode_mut(idx);
            inode.summary_mut().apply_diff(diff);
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

        let mut child_summaries = L::Summary::empty();

        match inode.children() {
            Either::Internal(inode_idxs) => {
                for &child_idx in inode_idxs {
                    let child = self.inode(child_idx);
                    assert_eq!(child.parent(), inode_idx);
                    assert_eq!(self.inode_height(child_idx) + 1, inode_height);
                    child_summaries += child.summary();
                }
            },

            Either::Leaf(leaf_idxs) => {
                assert_eq!(inode_height, 1);

                for &child_idx in leaf_idxs {
                    let child = self.lnode(child_idx);
                    assert_eq!(child.parent(), inode_idx);
                    child_summaries += child.value().summarize();
                }
            },
        }

        if !inode.summary().is_empty() {
            assert_eq!(child_summaries, inode.summary());
        }
    }

    /// Asserts that the summary offset of the given leaf is equal to the
    /// given summary.
    ///
    /// The summary offset of a leaf is the sum of the summaries of all the
    /// leaves that precede it in the Gtree, *not* including the summary of
    /// the leaf itself.
    fn assert_offset_of_leaf(
        &self,
        leaf_idx: LeafIdx<L>,
        leaf_offset: L::Length,
    ) {
        let mut summary = L::Length::zero();

        let mut parent_idx = self.lnode(leaf_idx).parent();

        let parent = self.inode(parent_idx);
        let child_idx = parent.idx_of_leaf_child(leaf_idx);
        summary += L::Length::len(
            &self.summary_offset_of_child(parent_idx, child_idx),
        );
        let mut inode_idx = parent_idx;
        parent_idx = parent.parent();

        while !parent_idx.is_dangling() {
            let parent = self.inode(parent_idx);
            let child_idx = parent.idx_of_internal_child(inode_idx);
            summary += L::Length::len(
                &self.summary_offset_of_child(parent_idx, child_idx),
            );
            inode_idx = parent_idx;
            parent_idx = parent.parent();
        }

        assert_eq!(summary, leaf_offset);
    }

    /// TODO: docs
    #[inline]
    fn bubble(
        &mut self,
        mut inode_idx: InodeIdx,
        mut maybe_split: Option<Inode<ARITY, L>>,
        mut patch: <L::Summary as Summary>::Diff,
        leaf_patch: <L::Summary as Summary>::Diff,
    ) {
        let mut parent_idx = self.inode(inode_idx).parent();

        loop {
            if parent_idx.is_dangling() {
                if let Some(split) = maybe_split {
                    self.root_has_split(split);
                }
                break;
            } else if let Some(split) = maybe_split {
                let split_summary = split.summary();
                let split_idx = self.push_inode(split, parent_idx);

                let parent = self.inode_mut(parent_idx);
                let old_summary = parent.summary();
                parent.summary_mut().apply_diff(patch);
                let idx_in_parent = parent.idx_of_internal_child(inode_idx);

                maybe_split = self.insert_in_inode(
                    parent_idx,
                    idx_in_parent + 1,
                    NodeIdx::from_internal(split_idx),
                    split_summary,
                );

                let parent = self.inode(parent_idx);
                patch = L::Summary::diff(old_summary, parent.summary());
                inode_idx = parent_idx;
                parent_idx = parent.parent();
            } else {
                self.apply_diff(parent_idx, leaf_patch);
                break;
            }
        }
    }

    /// Returns the index and summary offsets of the inode's child at the given
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
        debug_assert!(at_offset <= self.inode_len(of_inode));

        let mut offset = L::Length::zero();

        match self.inode(of_inode).children() {
            Either::Internal(inode_idxs) => {
                for (idx, &inode_idx) in inode_idxs.iter().enumerate() {
                    let len = self.inode_len(inode_idx);
                    offset += len;
                    if offset >= at_offset {
                        return (idx, offset - len);
                    }
                }
            },

            Either::Leaf(leaf_idxs) => {
                for (idx, &leaf_idx) in leaf_idxs.iter().enumerate() {
                    let len = self.leaf_len(leaf_idx);
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
        debug_assert!(range.end <= self.inode_len(of_inode));

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
            for (idx, child_summary) in iter.enumerate() {
                offset += child_summary;
                if offset > range.start {
                    return if offset >= range.end {
                        Some((idx, offset - child_summary))
                    } else {
                        None
                    };
                }
            }
            unreachable!();
        }

        match self.inode(of_inode).children() {
            Either::Internal(inode_idxs) => {
                let iter = inode_idxs.iter().map(|&idx| self.inode_len(idx));
                measure::<L, _>(iter, range)
            },

            Either::Leaf(leaf_idxs) => {
                let iter = leaf_idxs.iter().map(|&idx| self.leaf_len(idx));
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
            Either::Internal(inode_idx) => {
                L::Length::len(&self.inode(inode_idx).summary())
            },

            Either::Leaf(leaf_idx) => {
                L::Length::len(&self.leaf(leaf_idx).summarize())
            },
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
        let child_summary = match self.inode(of_inode).child(child_idx) {
            Either::Internal(inode_idx) => {
                let inode = self.inode_mut(inode_idx);
                mem::replace(inode.summary_mut(), L::Summary::empty())
            },
            Either::Leaf(leaf_idx) => {
                let leaf = self.leaf_mut(leaf_idx);
                let old_summary = leaf.summarize();
                leaf.delete();
                old_summary
            },
        };

        *self.inode_mut(of_inode).summary_mut() -= child_summary;
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
        debug_assert!(range.end - range.start <= self.leaf_len(leaf_idx));
        debug_assert_eq!(idx_in_parent, self.idx_of_leaf_in_parent(leaf_idx));

        let lnode = self.lnode_mut(leaf_idx);

        let old_summary = lnode.value().summarize();

        let (first, second) = delete_with(lnode.value_mut(), range);

        let patch = L::Summary::diff(old_summary, lnode.value().summarize());

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
                        let patch = L::Summary::diff(
                            L::Summary::empty(),
                            rest.summarize(),
                        );

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
                        let previous_len = self.leaf_len(previous_idx);
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
                        let previous_len = self.leaf_len(previous_idx);
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

        let old_summary = self.inode(idx).summary();

        let ((first, second), maybe_split) = delete::delete_range_in_inode(
            self,
            idx,
            range,
            delete_from,
            delete_up_to,
        );

        let patch = L::Summary::diff(old_summary, self.inode(idx).summary());

        let leaf_patch = L::Summary::diff(
            old_summary,
            self.inode(idx).summary()
                + maybe_split
                    .as_ref()
                    .map(Inode::summary)
                    .unwrap_or(L::Summary::empty()),
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

    #[inline(always)]
    fn inode_len(&self, idx: InodeIdx) -> L::Length {
        L::Length::len(&self.inode(idx).summary())
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
        let lnode = self.lnode_mut(leaf_idx);

        let old_summary = lnode.value().summarize();

        let (first, second) = insert_with(lnode.value_mut(), insert_at_offset);

        let patch = L::Summary::diff(old_summary, lnode.value().summarize());

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

        self.cursor = if insert_at_offset == L::Length::zero() {
            None
        } else if let Some(first_idx) = first_idx {
            let len = self.leaf_len(first_idx);

            let idx_in_parent = self.update_child_idx_after_inserting_after(
                parent_idx,
                idx_in_parent + 1,
            );

            Some(Cursor::new(first_idx, leaf_offset + len, idx_in_parent))
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
        child_summary: L::Summary,
    ) -> Option<Inode<ARITY, L>> {
        let min_children = Inode::<ARITY, L>::min_children();

        let inode = self.inode_mut(idx);

        debug_assert!(at_offset <= inode.len());

        if inode.is_full() {
            let split_offset = inode.len() - min_children;

            // Split so that the extra inode always has the minimum number
            // of children.
            let rest = if at_offset <= min_children {
                let rest = self.split_inode(idx, split_offset);
                self.inode_mut(idx).insert(at_offset, child, child_summary);
                rest
            } else {
                let mut rest = self.split_inode(idx, split_offset + 1);
                rest.insert(
                    at_offset - self.inode(idx).len(),
                    child,
                    child_summary,
                );
                rest
            };

            debug_assert_eq!(rest.len(), min_children);

            Some(rest)
        } else {
            inode.insert(at_offset, child, child_summary);
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
        let leaf_summary = leaf.summarize();

        let leaf_patch = L::Summary::diff(L::Summary::empty(), leaf_summary);

        let parent_idx = self.lnode(leaf_idx).parent();

        let inserted_idx = self.push_leaf(leaf, parent_idx);

        let parent = self.inode(parent_idx);
        let old_summary = parent.summary();
        let idx_in_parent = idx_in_parent;

        let maybe_split = self.insert_in_inode(
            parent_idx,
            idx_in_parent + 1,
            NodeIdx::from_leaf(inserted_idx),
            leaf_summary,
        );

        let parent = self.inode(parent_idx);
        let patch = L::Summary::diff(old_summary, parent.summary());

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
        mut first_summary: L::Summary,
        mut second_offset: usize,
        mut second_child: NodeIdx<L>,
        mut second_summary: L::Summary,
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
                first_summary,
                second_summary,
            ) = (
                second_child,
                first_child,
                second_offset,
                first_offset,
                second_summary,
                first_summary,
            )
        }

        let max_children = Inode::<ARITY, L>::max_children();

        let inode = self.inode_mut(idx);

        let len = inode.len();

        debug_assert!(second_offset <= inode.len());

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
                        first_summary,
                        second_offset,
                        second_child,
                        second_summary,
                    );
                    rest
                },

                Ordering::Less if first_offset >= split_offset + 2 => {
                    let mut rest = self.split_inode(idx, split_offset + 2);
                    let new_len = self.inode(idx).len();
                    first_offset -= new_len;
                    second_offset -= new_len;
                    rest.insert_two(
                        first_offset,
                        first_child,
                        first_summary,
                        second_offset,
                        second_child,
                        second_summary,
                    );
                    rest
                },

                _ => {
                    let mut rest = self.split_inode(idx, split_offset + 1);
                    let new_len = self.inode(idx).len();
                    rest.insert(
                        second_offset - new_len,
                        second_child,
                        second_summary,
                    );
                    self.inode_mut(idx).insert(
                        first_offset,
                        first_child,
                        first_summary,
                    );
                    rest
                },
            };

            debug_assert_eq!(rest.len(), min_children);

            Some(rest)
        } else {
            inode.insert_two(
                first_offset,
                first_child,
                first_summary,
                second_offset,
                second_child,
                second_summary,
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
        let first_summary = first_leaf.summarize();
        let second_summary = second_leaf.summarize();

        let leaf_patch = L::Summary::diff(
            L::Summary::empty(),
            first_summary + second_summary,
        );

        let parent_idx = self.lnode(leaf_idx).parent();

        let first_idx = self.push_leaf(first_leaf, parent_idx);
        let second_idx = self.push_leaf(second_leaf, parent_idx);

        let parent = self.inode(parent_idx);
        let old_summary = parent.summary();
        let idx_in_parent = idx_in_parent;

        let maybe_split = self.insert_two_in_inode(
            parent_idx,
            idx_in_parent + 1,
            NodeIdx::from_leaf(first_idx),
            first_summary,
            idx_in_parent + 1,
            NodeIdx::from_leaf(second_idx),
            second_summary,
        );

        let parent = self.inode(parent_idx);
        let patch = L::Summary::diff(old_summary, parent.summary());

        self.bubble(parent_idx, maybe_split, patch, leaf_patch);

        (first_idx, second_idx)
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
    fn leaf_len(&self, idx: LeafIdx<L>) -> L::Length {
        L::Length::len(&self.leaf(idx).summarize())
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

        if idx_in_parent + 1 == parent.len() {
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
        let split_summary = root_split.summary();

        let split_idx = self.push_inode(root_split, InodeIdx::dangling());

        let new_root = Inode::from_two_internals(
            self.root_idx,
            split_idx,
            self.summary(),
            split_summary,
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
        let summary = self.summary_offset_of_child(inode_idx, at_offset);
        self.inode_mut(inode_idx).split(at_offset, summary)
    }

    #[inline]
    fn summary_offset_of_child(
        &self,
        in_inode: InodeIdx,
        child_offset: ChildIdx,
    ) -> L::Summary {
        let mut summary = L::Summary::empty();

        match self.inode(in_inode).children() {
            Either::Internal(inode_idxs) => {
                for &idx in &inode_idxs[..child_offset] {
                    summary += self.inode(idx).summary();
                }
            },

            Either::Leaf(leaf_idxs) => {
                for &idx in &leaf_idxs[..child_offset] {
                    summary += self.leaf(idx).summarize();
                }
            },
        }

        summary
    }

    #[inline]
    fn update_child_idx_after_inserting_after(
        &self,
        old_parent: InodeIdx,
        old_idx: ChildIdx,
    ) -> ChildIdx {
        let parent = self.inode(old_parent);

        if parent.len() > old_idx {
            old_idx
        } else {
            old_idx - parent.len()
        }
    }

    /// TODO: docs
    #[inline]
    fn with_internal_mut<F, R>(&mut self, inode_idx: InodeIdx, fun: F) -> R
    where
        F: FnOnce(&mut Self) -> R,
    {
        debug_assert!(inode_idx != self.root_idx);

        let old_summary = self.inode(inode_idx).summary();

        let ret = fun(self);

        let inode = self.inode(inode_idx);

        let new_summary = inode.summary();

        let parent_idx = inode.parent();

        debug_assert!(!parent_idx.is_dangling());

        let parent = self.inode_mut(parent_idx);
        *parent.summary_mut() -= old_summary;
        *parent.summary_mut() += new_summary;

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
            let summary = split.summary();
            let split_idx = self.push_inode(split, parent_idx);
            self.insert_in_inode(
                parent_idx,
                idx_in_parent + 1,
                NodeIdx::from_internal(split_idx),
                summary,
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

        let old_summary = leaf.summarize();

        let ret = fun(leaf);

        let new_summary = leaf.summarize();

        let parent_idx = lnode.parent();

        let parent = self.inode_mut(parent_idx);
        *parent.summary_mut() -= old_summary;
        *parent.summary_mut() += new_summary;

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
                let left_summary = left.summarize();
                let left_idx = self.push_leaf(left, parent_idx);

                let right_summary = right.summarize();
                let right_idx = self.push_leaf(right, parent_idx);

                let split = self.insert_two_in_inode(
                    parent_idx,
                    insert_at,
                    NodeIdx::from_leaf(left_idx),
                    left_summary,
                    insert_at,
                    NodeIdx::from_leaf(right_idx),
                    right_summary,
                );

                (Some(left_idx), Some(right_idx), split)
            },

            (Some(left), None) => {
                let summary = left.summarize();
                let left_idx = self.push_leaf(left, parent_idx);

                let split = self.insert_in_inode(
                    parent_idx,
                    insert_at,
                    NodeIdx::from_leaf(left_idx),
                    summary,
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
    /// The total summary of this node, which is the sum of the summaries of
    /// all of its children.
    summary: L::Summary,

    /// The index of this node's parent, or `InodeIdx::dangling()` if this is
    /// the root node.
    parent: InodeIdx,

    /// The number of children this inode is storing, which is always less than
    /// or equal to `ARITY`.
    len: usize,

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
        let children = &self.children[..self.len];

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
    fn from_leaf(
        leaf: LeafIdx<L>,
        summary: L::Summary,
        parent: InodeIdx,
    ) -> Self {
        let mut children = [NodeIdx::dangling(); ARITY];
        children[0] = NodeIdx::from_leaf(leaf);
        Self { children, parent, summary, has_leaves: true, len: 1 }
    }

    /// Creates a new internal node containing the two given inodes.
    #[inline]
    fn from_two_internals(
        first: InodeIdx,
        second: InodeIdx,
        first_summary: L::Summary,
        second_summary: L::Summary,
        parent: InodeIdx,
    ) -> Self {
        let mut children = [NodeIdx::dangling(); ARITY];
        children[0] = NodeIdx::from_internal(first);
        children[1] = NodeIdx::from_internal(second);

        Self {
            children,
            parent,
            summary: first_summary + second_summary,
            has_leaves: false,
            len: 2,
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
        first_summary: L::Summary,
        second_offset: usize,
        second_child: NodeIdx<L>,
        second_summary: L::Summary,
    ) {
        debug_assert!(first_offset <= second_offset);
        debug_assert!(second_offset <= self.len());
        debug_assert!(Self::max_children() - self.len() >= 2);

        self.insert(first_offset, first_child, first_summary);
        self.insert(second_offset + 1, second_child, second_summary);
    }

    /// Inserts a child into this inode at the given offset.
    ///
    /// Panics if this inode is already full or if the offset is out of bounds.
    #[inline]
    fn insert(
        &mut self,
        at_offset: usize,
        child: NodeIdx<L>,
        child_summary: L::Summary,
    ) {
        debug_assert!(at_offset <= self.len());
        debug_assert!(!self.is_full());

        self.children[at_offset..].rotate_right(1);
        self.children[at_offset] = child;
        self.summary += child_summary;
        self.len += 1;
    }

    #[inline]
    fn is_full(&self) -> bool {
        self.len() == Self::max_children()
    }

    #[inline(always)]
    fn len(&self) -> usize {
        self.len
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
    /// The `new_summary` parameter is the sum of the summaries of the first
    /// `at_offset` children.
    #[inline]
    fn split(&mut self, at_offset: usize, new_summary: L::Summary) -> Self {
        let len = self.len() - at_offset;

        let mut children = [NodeIdx::dangling(); ARITY];
        children[..len].copy_from_slice(&self.children[at_offset..self.len()]);

        let summary = self.summary - new_summary;

        self.summary = new_summary;

        self.len = at_offset;

        Self {
            children,
            has_leaves: self.has_leaves,
            len,
            parent: InodeIdx::dangling(),
            summary,
        }
    }

    #[inline(always)]
    fn summary(&self) -> L::Summary {
        self.summary
    }

    #[inline(always)]
    fn summary_mut(&mut self) -> &mut L::Summary {
        &mut self.summary
    }
}

/// A leaf node of the Gtree.
#[derive(Clone)]
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
                    let leaf_summary = gtree.leaf_len(leaf_idx);

                    let idx_in_parent = gtree
                        .update_child_idx_after_inserting_after(
                            in_inode,
                            child_idx + 1,
                        );

                    Some(Cursor::new(
                        idx,
                        leaf_offset + leaf_summary,
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

        let mut idxs = 0..gtree.inode(idx).len();
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
                            let summary = extra.summary();
                            let inode_idx = gtree.push_inode(extra, idx);
                            let node_idx = NodeIdx::from_internal(inode_idx);
                            extra_from_start = Some((node_idx, summary));
                        }
                    },

                    Either::Leaf(leaf_idx) => {
                        let split = gtree.with_leaf_mut(leaf_idx, |leaf| {
                            del_from(leaf, range.start - offset)
                        });

                        if let Some(split) = split {
                            let summary = split.summarize();
                            let leaf_idx = gtree.push_leaf(split, idx);
                            leaf_idx_start = Some(leaf_idx);
                            let node_idx = NodeIdx::from_leaf(leaf_idx);
                            extra_from_start = Some((node_idx, summary));
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
                            let summary = extra.summary();
                            let inode_idx = gtree.push_inode(extra, idx);
                            let node_idx = NodeIdx::from_internal(inode_idx);
                            extra_from_end = Some((node_idx, summary));
                        }
                    },

                    Either::Leaf(leaf_idx) => {
                        let split = gtree.with_leaf_mut(leaf_idx, |leaf| {
                            del_up_to(leaf, range.end - offset)
                        });

                        if let Some(split) = split {
                            let summary = split.summarize();
                            let leaf_idx = gtree.push_leaf(split, idx);

                            leaf_idx_end = Some(leaf_idx);

                            let node_idx = NodeIdx::from_leaf(leaf_idx);
                            extra_from_end = Some((node_idx, summary));
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
            (Some((start, start_summary)), Some((end, end_summary))) => gtree
                .insert_two_in_inode(
                    idx,
                    start_offset,
                    start,
                    start_summary,
                    end_offset,
                    end,
                    end_summary,
                ),

            (Some((start, summary)), None) => {
                gtree.insert_in_inode(idx, start_offset, start, summary)
            },

            (None, Some((end, summary))) => {
                gtree.insert_in_inode(idx, end_offset, end, summary)
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
        let len = gtree.inode(inode_idx).len();

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

        for child_idx in 0..gtree.inode(idx).len() {
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

            write!(f, "{:?} @ ", self.summary())?;

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

    struct DebugLevel<'a, 'b, const N: usize, L: Leaf> {
        gtree: &'a Gtree<N, L>,
        inode_idx: InodeIdx,
        shifts: &'b mut String,
        ident: &'a str,
        last_shift_byte_len: usize,
    }

    fn indent(f: &mut Formatter, level: usize) -> FmtResult {
        for _ in 0..level {
            write!(f, "    ")?;
        }
        Ok(())
    }

    impl<const N: usize, L: Leaf> Debug for DebugLevel<'_, '_, N, L> {
        fn fmt(&self, f: &mut Formatter) -> FmtResult {
            let inode = self.gtree.inode(self.inode_idx);

            let indent_level = f.precision().unwrap_or(0);

            if indent_level == 1 {
                indent(f, indent_level)?;
            }

            writeln!(
                f,
                "{}{}{:?}",
                &self.shifts[..self.shifts.len() - self.last_shift_byte_len],
                self.ident,
                inode.summary()
            )?;

            let is_last = |idx: usize| idx + 1 == inode.len();

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

                        let mut shifts = self.shifts.clone();
                        shifts.push_str(shift);

                        let level = DebugLevel {
                            gtree: self.gtree,
                            inode_idx,
                            shifts: &mut shifts,
                            ident: ident(i),
                            last_shift_byte_len: shift.len(),
                        };

                        f.write_fmt(format_args!(
                            "{:.precision$?}",
                            level,
                            precision = indent_level + 1
                        ))?;
                    }
                },

                Either::Leaf(leaf_idxs) => {
                    for (i, &leaf_idx) in leaf_idxs.iter().enumerate() {
                        if let Some(cursor) = self.gtree.cursor {
                            if cursor.leaf_idx == leaf_idx {
                                indent(f, indent_level)?;
                                writeln!(f, "{}│", self.shifts)?;
                                indent(f, indent_level)?;
                                writeln!(
                                    f,
                                    "{}│ -> cursor @ {:?}",
                                    self.shifts, cursor.offset
                                )?;
                                indent(f, indent_level)?;
                                writeln!(f, "{}│", self.shifts)?;
                            }
                        }

                        let ident = ident(i);
                        let lnode = self.gtree.lnode(leaf_idx);
                        indent(f, indent_level)?;
                        writeln!(
                            f,
                            "{}{}{:#.precision$?}",
                            self.shifts,
                            ident,
                            &lnode.value(),
                            precision = indent_level,
                        )?;
                    }
                },
            }

            Ok(())
        }
    }

    impl<const N: usize, L: Leaf> Debug for DebugAsBtree<'_, N, L> {
        fn fmt(&self, f: &mut Formatter) -> FmtResult {
            if self.gtree.is_initialized() {
                let level = DebugLevel {
                    gtree: self.gtree,
                    inode_idx: self.inode_idx,
                    shifts: &mut String::new(),
                    ident: "",
                    last_shift_byte_len: 0,
                };

                let precision = f.precision().unwrap_or(0);

                if precision == 0 {
                    writeln!(f)?;
                }

                f.write_fmt(format_args!(
                    "{:.precision$?}",
                    level,
                    precision = precision + 1
                ))
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
                    if *child_idx < self.gtree.inode(*idx).len() {
                        break;
                    }
                    self.path.pop();
                }

                let (mut idx, _) = self.path.last()?;

                loop {
                    match self.gtree.inode(idx).children() {
                        Either::Internal(inode_idxs) => {
                            let first_idx = inode_idxs[0];
                            self.path.push((first_idx, 0));
                            idx = first_idx;
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

    impl Summarize for TestLeaf {
        type Summary = u64;

        fn summarize(&self) -> Self::Summary {
            self.len * (self.is_visible as u64)
        }
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
