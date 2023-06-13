use core::fmt::Debug;
use core::ops::{Add, AddAssign, Index, IndexMut, Sub, SubAssign};

/// TODO: docs
pub trait Summarize: Debug + Sized + 'static {
    type Length: Debug
        + Default
        + Copy
        + Add<Self::Length, Output = Self::Length>
        + AddAssign<Self::Length>
        + Sub<Self::Length, Output = Self::Length>
        + SubAssign<Self::Length>
        + PartialOrd<Self::Length>
        + PartialEq<Self::Length>;

    fn summarize(&self) -> Self::Length;
}

/// A grow-only tree.
///
/// TODO: docs
#[derive(Debug, Clone)]
pub struct Gtree<const ARITY: usize, Leaf: Summarize> {
    /// TODO: docs
    nodes: Vec<Inode<ARITY, Leaf>>,

    /// TODO: docs
    root_idx: GtreeIdx,
}

/// TODO: docs
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GtreeIdx(usize);

impl GtreeIdx {
    #[inline]
    const fn dangling() -> Self {
        Self(usize::MAX)
    }

    #[inline]
    fn is_dangling(self) -> bool {
        self == Self::dangling()
    }
}

impl<const ARITY: usize, Leaf: Summarize> Index<GtreeIdx>
    for Gtree<ARITY, Leaf>
{
    type Output = Inode<ARITY, Leaf>;

    #[inline]
    fn index(&self, idx: GtreeIdx) -> &Self::Output {
        &self.nodes[idx.0]
    }
}

impl<const ARITY: usize, Leaf: Summarize> IndexMut<GtreeIdx>
    for Gtree<ARITY, Leaf>
{
    #[inline]
    fn index_mut(&mut self, idx: GtreeIdx) -> &mut Self::Output {
        &mut self.nodes[idx.0]
    }
}

impl<const ARITY: usize, Leaf: Summarize> Gtree<ARITY, Leaf> {
    /// TODO: docs
    #[inline]
    pub fn new(first_leaf: Leaf) -> Self {
        let root_idx = GtreeIdx(0);

        let root = Inode::first_root(first_leaf);

        let mut nodes = Vec::with_capacity(256);

        nodes.push(root);

        Self { nodes, root_idx }
    }

    #[inline]
    fn root(&self) -> &Inode<ARITY, Leaf> {
        &self[self.root_idx]
    }

    #[inline]
    fn child_lengths(
        &self,
        child_idx: GtreeIdx,
    ) -> impl Iterator<Item = Leaf::Length> + '_ {
        core::iter::empty()
    }

    #[inline]
    fn inode(&mut self, idx: GtreeIdx) -> &mut Inode<ARITY, Leaf> {
        &mut self.nodes[idx.0]
    }

    #[inline]
    fn push(&mut self, inode: Inode<ARITY, Leaf>) -> GtreeIdx {
        assert!(self.nodes.len() < usize::MAX);
        self.nodes.push(inode);
        GtreeIdx(self.nodes.len())
    }

    #[inline]
    pub fn len(&self) -> Leaf::Length {
        self.root().summary
    }

    /// TODO: docs
    #[inline]
    fn root_has_split(&mut self, root_split: Inode<ARITY, Leaf>) {
        let summary = self.len() + root_split.len();
        let other_idx = self.push(root_split);
        let new_root = Inode::new_root(self.root_idx, other_idx, summary);
        self.root_idx = self.push(new_root);
    }

    /// TODO: docs
    #[inline]
    pub fn with_leaf_mut<F: FnOnce(&mut Leaf) -> Option<Leaf>>(
        &mut self,
        at_summary: Leaf::Length,
        fun: F,
    ) {
        if let Some(root_split) = tree_traversal::recurse_to_leaf(
            self,
            self.root_idx,
            at_summary,
            fun,
        ) {
            self.root_has_split(root_split)
        }
    }
}

/// TODO: docs
struct Inode<const ARITY: usize, Leaf: Summarize> {
    /// TODO: docs
    summary: Leaf::Length,

    /// TODO: docs
    parent: GtreeIdx,

    /// TODO: docs
    has_leaves: bool,

    /// TODO: docs
    children: Children<ARITY, NodePtr<Leaf>>,
}

impl<const ARITY: usize, Leaf: Summarize> Inode<ARITY, Leaf> {
    /// TODO: docs
    #[inline]
    fn child(&mut self, child_idx: usize) -> Either<GtreeIdx, &mut Leaf> {
        let child = &mut self.children.as_mut_slice()[child_idx];

        if self.has_leaves {
            // SAFETY: all the children are leaf nodes.
            Either::Right(unsafe { child.as_leaf_mut() })
        } else {
            // SAFETY: all the children are internal indexes.
            Either::Left(unsafe { child.as_idx() })
        }
    }

    /// TODO: docs
    #[inline]
    fn first_root(first_leaf: Leaf) -> Self {
        let len = first_leaf.summarize();

        let mut children = Children::new();

        let leaf_ptr = NodePtr::new_to_leaf(first_leaf);

        // SAFETY: the children are still empty so definitely not full.
        unsafe { children.push(leaf_ptr) };

        Self {
            summary: len,
            parent: GtreeIdx::dangling(),
            has_leaves: true,
            children,
        }
    }

    /// TODO: docs
    fn insert(
        &mut self,
        at_offset: usize,
        child: NodePtr<Leaf>,
        child_len: Leaf::Length,
    ) -> Option<Self> {
        debug_assert!(at_offset <= self.children.len());

        if self.is_full() {
            let split_offset = self.children.len() - Self::min_children();

            // Split so that the extra inode always has the minimum number of
            // children.
            let rest = if at_offset <= Self::min_children() {
                let rest = self.split(split_offset);
                self.insert(at_offset, child, child_len);
                rest
            } else {
                let mut rest = self.split(split_offset + 1);
                rest.insert(at_offset - self.children.len(), child, child_len);
                rest
            };

            debug_assert_eq!(rest.children.len(), Self::min_children());

            Some(rest)
        } else {
            self.summary += child_len;
            // SAFETY: Self is not full.
            unsafe { self.children.insert(at_offset, child) };
            None
        }
    }

    #[inline]
    fn insert_leaf(&mut self, at_offset: usize, leaf: Leaf) -> Option<Self> {
        debug_assert!(self.has_leaves);
        let summary = leaf.summarize();
        self.insert(at_offset, NodePtr::new_to_leaf(leaf), summary)
    }

    #[inline]
    fn insert_internal_idx(
        &mut self,
        at_offset: usize,
        idx: GtreeIdx,
        len: Leaf::Length,
    ) -> Option<Self> {
        debug_assert!(!self.has_leaves);
        self.insert(at_offset, NodePtr::new_to_idx(idx), len)
    }

    #[inline]
    fn is_full(&self) -> bool {
        self.children.is_full()
    }

    #[inline]
    fn len(&self) -> Leaf::Length {
        self.summary
    }

    #[inline]
    const fn min_children() -> usize {
        ARITY / 2
    }

    #[inline]
    fn new_root(
        old_root: GtreeIdx,
        root_split: GtreeIdx,
        len: Leaf::Length,
    ) -> Self {
        let mut children = Children::new();

        // SAFETY: the children are still empty so definitely not full.
        unsafe {
            children.push(NodePtr::new_to_idx(old_root));
            children.push(NodePtr::new_to_idx(root_split));
        };

        Self {
            summary: len,
            parent: GtreeIdx::dangling(),
            has_leaves: false,
            children,
        }
    }

    #[inline]
    fn split(&mut self, at_offset: usize) -> Self {
        let other_children = unsafe { self.children.split(at_offset) };

        // TODO: adjust summaries

        todo!();
    }
}

impl<const ARITY: usize, Leaf: Summarize + Debug> Debug
    for Inode<ARITY, Leaf>
{
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        todo!();
    }
}

impl<const ARITY: usize, Leaf: Summarize + Clone> Clone
    for Inode<ARITY, Leaf>
{
    #[inline]
    fn clone(&self) -> Self {
        let mut new_children = Children::new();

        if self.has_leaves {
            for child in self.children.as_slice() {
                // SAFETY: this inode stores leaves so the child points to a
                // leaf that's never been dropped.
                let leaf = unsafe { child.as_leaf() };
                let ptr = NodePtr::new_to_leaf(leaf.clone());
                unsafe { new_children.push(ptr) };
            }
        } else {
            for child in self.children.as_slice() {
                // SAFETY: this inode stores leaves so the child points to a
                // leaf that's never been dropped.
                let idx = unsafe { child.as_idx() };
                let ptr = NodePtr::new_to_idx(idx);
                unsafe { new_children.push(ptr) };
            }
        }

        Self {
            summary: self.summary,
            parent: self.parent,
            has_leaves: self.has_leaves,
            children: new_children,
        }
    }
}

impl<const ARITY: usize, Leaf: Summarize> Drop for Inode<ARITY, Leaf> {
    #[inline]
    fn drop(&mut self) {
        if self.has_leaves {
            for child in self.children.as_mut_slice() {
                // SAFETY: this inode stores leaves so the child points to a
                // leaf that's never been dropped.
                unsafe {
                    core::ptr::drop_in_place(child.as_leaf_mut());
                }
            }
        }
    }
}

use children::Children;

mod children {
    use core::mem::{transmute, MaybeUninit};
    use core::ptr;

    pub(super) struct Children<const MAX_CHILDREN: usize, T> {
        len: usize,
        children: [MaybeUninit<T>; MAX_CHILDREN],
    }

    impl<const MAX_CHILDREN: usize, T> Children<MAX_CHILDREN, T> {
        #[inline]
        pub fn as_slice(&self) -> &[T] {
            // SAFETY: the first `len` children are initialized.
            unsafe { transmute(&self.children[..self.len]) }
        }

        #[inline]
        pub fn as_mut_slice(&mut self) -> &mut [T] {
            // SAFETY: the first `len` children are initialized.
            unsafe { transmute(&mut self.children[..self.len]) }
        }

        /// SAFETY: Self must not be full and the offset must be within bounds
        /// (i.e. <= than Self's length).
        #[inline]
        pub unsafe fn insert(&mut self, at_offset: usize, child: T) {
            debug_assert!(!self.is_full());
            debug_assert!(at_offset <= self.len);

            let ptr = self.children.as_mut_ptr();

            // Move all the following items one slot to the right.
            unsafe {
                ptr::copy(
                    ptr.add(at_offset),
                    ptr.add(at_offset + 1),
                    self.len() - at_offset,
                );
            };

            self.children[at_offset].write(child);

            self.len += 1;
        }

        #[inline]
        pub fn is_empty(&self) -> bool {
            self.len == 0
        }

        #[inline]
        pub fn is_full(&self) -> bool {
            self.len == MAX_CHILDREN
        }

        #[inline]
        pub fn len(&self) -> usize {
            self.len
        }

        #[inline]
        pub fn new() -> Self {
            Self {
                len: 0,
                // SAFETY: An uninitialized `[MaybeUninit<_>; N]` is valid.
                children: unsafe { MaybeUninit::uninit().assume_init() },
            }
        }

        #[inline(always)]
        pub unsafe fn push(&mut self, child: T) {
            debug_assert!(!self.is_full());
            self.children[self.len].write(child);
            self.len += 1;
        }

        /// SAFETY: The offset must be within bounds (i.e. <= than Self's
        /// length).
        ///
        /// Note that if the offset is exactly equal to Self's length the
        /// returned Children will be empty.
        #[inline]
        pub unsafe fn split(&mut self, at_offset: usize) -> Self {
            debug_assert!(at_offset <= self.len);

            let mut split = Self::new();

            // Move all the following items one slot to the right.
            unsafe {
                ptr::copy(
                    self.children.as_ptr().add(at_offset),
                    split.children.as_mut_ptr(),
                    self.len() - at_offset,
                );
            };

            self.len = at_offset;

            split
        }
    }
}

use node_ptr::NodePtr;

mod node_ptr {
    use core::marker::PhantomData;
    use core::ptr::NonNull;

    use super::*;

    /// TODO: docs
    pub(super) union NodePtr<Leaf: 'static> {
        /// TODO: docs
        to_internal: GtreeIdx,

        /// TODO: docs
        to_leaf: NonNull<Leaf>,

        /// Informs the compiler that this type owns the leaf. Used during
        /// [drop check] analysis.
        ///
        /// [drop check]: https://doc.rust-lang.org/std/marker/struct.PhantomData.html#ownership-and-the-drop-check
        owning_marker: PhantomData<Leaf>,
    }

    impl<Leaf: 'static> NodePtr<Leaf> {
        #[inline]
        pub unsafe fn as_idx(&self) -> GtreeIdx {
            self.to_internal
        }

        #[inline]
        pub unsafe fn as_leaf(&self) -> &Leaf {
            self.to_leaf.as_ref()
        }

        #[inline]
        pub unsafe fn as_leaf_mut(&mut self) -> &mut Leaf {
            self.to_leaf.as_mut()
        }

        #[inline]
        pub fn new_to_idx(tree_idx: GtreeIdx) -> Self {
            Self { to_internal: tree_idx }
        }

        #[inline]
        pub fn new_to_leaf(leaf: Leaf) -> Self {
            let ptr = Box::leak::<'static>(Box::new(leaf));
            Self { to_leaf: unsafe { NonNull::new_unchecked(ptr) } }
        }
    }

    impl<Leaf: Summarize> NodePtr<Leaf> {
        #[inline]
        pub unsafe fn as_internal<'this, 'tree: 'this, const ARITY: usize>(
            &'this self,
            gtree: &'tree Gtree<ARITY, Leaf>,
        ) -> &'this Inode<ARITY, Leaf> {
            &gtree[self.to_internal]
        }

        #[inline]
        pub unsafe fn as_internal_mut<
            'this,
            'tree: 'this,
            const ARITY: usize,
        >(
            &'this mut self,
            gtree: &'tree mut Gtree<ARITY, Leaf>,
        ) -> &'this mut Inode<ARITY, Leaf> {
            &mut gtree[self.to_internal]
        }
    }
}

mod tree_traversal {
    use super::*;

    pub(super) fn recurse_to_leaf<const N: usize, L, F>(
        gtree: &mut Gtree<N, L>,
        idx: GtreeIdx,
        at_offset: L::Length,
        fun: F,
    ) -> Option<Inode<N, L>>
    where
        L: Summarize,
        F: FnOnce(&mut L) -> Option<L>,
    {
        let mut offset = L::Length::default();

        let mut child_idx = 0;

        for (idx, child_len) in gtree.child_lengths(idx).enumerate() {
            offset += child_len;

            if offset >= at_offset {
                offset -= child_len;
                child_idx = idx;
                break;
            }
        }

        match gtree.inode(idx).child(child_idx) {
            Either::Left(next_idx) => {
                let maybe_split =
                    recurse_to_leaf(gtree, next_idx, at_offset - offset, fun);

                if let Some(split) = maybe_split {
                    // TODO: when am I updating split's parent?
                    let len = split.len();
                    let pushed_idx = gtree.push(split);
                    let inode = gtree.inode(idx);
                    inode.insert_internal_idx(child_idx + 1, pushed_idx, len)
                } else {
                    None
                }
            },

            Either::Right(leaf) => {
                if let Some(other) = fun(leaf) {
                    let inode = gtree.inode(idx);
                    inode.insert_leaf(child_idx + 1, other)
                } else {
                    None
                }
            },
        }
    }
}

use either::Either;

mod either {
    pub(super) enum Either<Left, Right> {
        Left(Left),
        Right(Right),
    }
}
