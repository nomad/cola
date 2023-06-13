use core::fmt::Debug;
use core::ops::{Add, AddAssign, Index, IndexMut, Sub, SubAssign};

/// TODO: docs
pub trait Summarize: Debug + Sized + 'static {
    type Summary: Debug
        + Default
        + Copy
        + Add<Self::Summary, Output = Self::Summary>
        + AddAssign<Self::Summary>
        + Sub<Self::Summary, Output = Self::Summary>
        + SubAssign<Self::Summary>
        + PartialEq<Self::Summary>;

    fn summarize(&self) -> Self::Summary;
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

        let root = Inode::new_from_leaf(first_leaf, GtreeIdx::dangling());

        let mut nodes = Vec::with_capacity(256);

        nodes.push(root);

        Self { nodes, root_idx }
    }

    #[inline]
    fn root(&self) -> &Inode<ARITY, Leaf> {
        &self[self.root_idx]
    }

    #[inline]
    pub fn summary(&self) -> Leaf::Summary {
        self.root().summary
    }
}

/// TODO: docs
struct Inode<const ARITY: usize, Leaf: Summarize> {
    /// TODO: docs
    summary: Leaf::Summary,

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
    fn new_from_leaf(leaf: Leaf, parent: GtreeIdx) -> Self {
        let summary = leaf.summarize();

        let mut children = Children::new();

        let leaf_ptr = NodePtr::new_to_leaf(leaf);

        // SAFETY: the children are still empty so definitely not full.
        unsafe { children.push(leaf_ptr) };

        Self { summary, parent, has_leaves: true, children }
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

        // SAFETY: Self must not be full and the offset must be within bounds
        // (i.e. <= than Self's length).
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

        // SAFETY: The offset must be within bounds (i.e. <= than Self's
        // length).
        //
        // Note that if the offset is exactly equal to Self's length the
        // returned Children will be empty.
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
