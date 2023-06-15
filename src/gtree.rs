use core::fmt::Debug;
use core::ops::{Add, AddAssign, Range, Sub, SubAssign};

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
#[derive(Clone)]
pub struct Gtree<const ARITY: usize, Leaf: Summarize> {
    /// TODO: docs
    inodes: Vec<Inode<ARITY, Leaf>>,

    /// TODO: docs
    root_idx: GtreeIdx,
}

/// TODO: docs
#[derive(Clone, Copy, PartialEq, Eq)]
struct GtreeIdx(usize);

impl Debug for GtreeIdx {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "GtreeIdx({})", self.0)
    }
}

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GtreeLeafIdx {
    row: GtreeIdx,
    col: usize,
}

impl<const ARITY: usize, Leaf: Summarize + Debug> Debug
    for Gtree<ARITY, Leaf>
{
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        struct GtreeKey {
            idx: usize,
            is_root: bool,
        }

        impl Debug for GtreeKey {
            fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
                let prefix = if self.is_root { "R -> " } else { "" };
                write!(f, "{prefix}{}", self.idx)
            }
        }

        let mut dbg = f.debug_map();

        for (idx, inode) in self.inodes.iter().enumerate() {
            let key = GtreeKey { idx, is_root: idx == self.root_idx.0 };
            dbg.entry(&key, inode);
        }

        dbg.finish()
    }
}

impl<const ARITY: usize, Leaf: Summarize> Gtree<ARITY, Leaf> {
    #[inline]
    fn inode(&self, idx: GtreeIdx) -> &Inode<ARITY, Leaf> {
        &self.inodes[idx.0]
    }

    #[inline]
    fn inode_mut(&mut self, idx: GtreeIdx) -> &mut Inode<ARITY, Leaf> {
        &mut self.inodes[idx.0]
    }

    #[inline]
    pub fn len(&self) -> Leaf::Length {
        self.root().len()
    }

    /// TODO: docs
    #[inline]
    pub fn new(first_leaf: Leaf) -> Self {
        let root = Inode::first_root(first_leaf);
        let mut nodes = Vec::with_capacity(256);
        nodes.push(root);
        Self { inodes: nodes, root_idx: GtreeIdx(0) }
    }

    /// TODO: docs
    #[inline]
    fn push(
        &mut self,
        mut inode: Inode<ARITY, Leaf>,
        parent_idx: GtreeIdx,
    ) -> GtreeIdx {
        *inode.parent_mut() = parent_idx;
        let idx = GtreeIdx(self.inodes.len());
        assert!(!idx.is_dangling());
        self.inodes.push(inode);
        idx
    }

    #[inline]
    fn root(&self) -> &Inode<ARITY, Leaf> {
        self.inode(self.root_idx)
    }

    /// TODO: docs
    #[inline]
    fn root_has_split(&mut self, root_split: Inode<ARITY, Leaf>) {
        let split_len = root_split.len();
        let split_idx = self.push(root_split, GtreeIdx::dangling());
        let new_root = Inode::new_root(
            (self.root_idx, self.root().len()),
            (split_idx, split_len),
        );
        let new_root_idx = self.push(new_root, GtreeIdx::dangling());
        *self.root_mut().parent_mut() = new_root_idx;
        *self.inode_mut(split_idx).parent_mut() = new_root_idx;
        self.root_idx = new_root_idx;
    }

    #[inline]
    fn root_mut(&mut self) -> &mut Inode<ARITY, Leaf> {
        self.inode_mut(self.root_idx)
    }

    /// TODO: docs
    #[inline]
    pub fn insert_at_offset<F>(&mut self, offset: Leaf::Length, fun: F)
    where
        F: FnOnce(&mut Leaf, Leaf::Length) -> (Option<Leaf>, Option<Leaf>),
    {
        if let Some(root_split) =
            insert::insert_at_offset(self, self.root_idx, offset, fun)
        {
            self.root_has_split(root_split)
        }
    }

    /// TODO: docs
    #[inline]
    pub fn delete_range<DelLeaf, DelFrom, DelUpTo, DelRange>(
        &mut self,
        range: Range<Leaf::Length>,
        mut delete_leaf: DelLeaf,
        delete_from: DelFrom,
        delete_up_to: DelUpTo,
        delete_range: DelRange,
    ) where
        DelLeaf: FnMut(&mut Leaf),
        DelFrom: FnOnce(&mut Leaf, Leaf::Length) -> Option<Leaf>,
        DelUpTo: FnOnce(&mut Leaf, Leaf::Length) -> Option<Leaf>,
        DelRange: FnOnce(
            &mut Leaf,
            Range<Leaf::Length>,
        ) -> (Option<Leaf>, Option<Leaf>),
    {
        if let Some(root_split) = delete::delete_range(
            self,
            self.root_idx,
            range,
            &mut delete_leaf,
            delete_from,
            delete_up_to,
            delete_range,
        ) {
            self.root_has_split(root_split)
        }
    }
}

enum Either<Left, Right> {
    Left(Left),
    Right(Right),
}

use inode::Inode;

mod inode {
    use super::*;

    /// TODO: docs
    pub(super) struct Inode<const ARITY: usize, Leaf: Summarize> {
        /// TODO: docs
        summary: Leaf::Length,

        /// TODO: docs
        parent: GtreeIdx,

        /// TODO: docs
        has_leaves: bool,

        /// TODO: docs
        children: Children<ARITY, (Leaf::Length, NodePtr<Leaf>)>,
    }

    impl<const ARITY: usize, Leaf: Summarize> Inode<ARITY, Leaf> {
        /// TODO: docs
        #[inline]
        pub fn child_at_offset<const WITH_RIGHT_BIAS: bool>(
            &self,
            at_offset: Leaf::Length,
        ) -> (usize, Leaf::Length) {
            let cmp = if WITH_RIGHT_BIAS {
                Leaf::Length::gt
            } else {
                Leaf::Length::ge
            };

            let mut offset = Leaf::Length::default();

            let children = self.children();

            for (idx, &(child_len, _)) in children.iter().enumerate() {
                offset += child_len;

                if cmp(&offset, &at_offset) {
                    return (idx, offset - child_len);
                }
            }

            unreachable!();
        }

        /// TODO: docs
        #[inline]
        pub fn child_containing_range(
            &self,
            range: Range<Leaf::Length>,
        ) -> Option<(usize, Leaf::Length)> {
            let mut offset = Leaf::Length::default();

            let children = self.children();

            for (idx, &(child_len, _)) in children.iter().enumerate() {
                offset += child_len;

                if offset > range.start {
                    if offset >= range.end {
                        return Some((idx, offset - child_len));
                    } else {
                        return None;
                    }
                }
            }

            unreachable!();
        }

        pub fn delete_children(
            &mut self,
            idx_range: Range<usize>,
            _del_leaf: &mut impl FnOnce(&mut Leaf),
        ) {
            for (child_len, _) in &mut self.children.as_mut_slice()[idx_range]
            {
                let old_len = core::mem::take(child_len);
                self.summary -= old_len;
            }
        }

        /// TODO: docs
        #[inline]
        pub fn child_mut(
            &mut self,
            child_idx: usize,
        ) -> Either<GtreeIdx, &mut Leaf> {
            let (_, child) = &mut self.children.as_mut_slice()[child_idx];

            if self.has_leaves {
                // SAFETY: all the children are leaf nodes.
                Either::Right(unsafe { child.as_leaf_mut() })
            } else {
                // SAFETY: all the children are internal indexes.
                Either::Left(unsafe { child.as_idx() })
            }
        }

        #[inline]
        pub fn update_summary(
            &mut self,
            child_idx: usize,
            new_summary: Leaf::Length,
        ) {
            let old_summary = &mut self.children.as_mut_slice()[child_idx].0;
            self.summary -= *old_summary;
            self.summary += new_summary;
            *old_summary = new_summary;
        }

        #[inline]
        pub fn children(&self) -> &[(Leaf::Length, NodePtr<Leaf>)] {
            self.children.as_slice()
        }

        /// TODO: docs
        #[inline]
        pub fn first_root(first_leaf: Leaf) -> Self {
            let len = first_leaf.summarize();

            let mut children = Children::new();

            let leaf_ptr = NodePtr::from_leaf(first_leaf);

            // SAFETY: the children are still empty so definitely not full.
            unsafe { children.push((len, leaf_ptr)) };

            Self {
                summary: len,
                parent: GtreeIdx::dangling(),
                has_leaves: true,
                children,
            }
        }

        /// TODO: docs
        pub fn insert(
            &mut self,
            at_offset: usize,
            child: NodePtr<Leaf>,
            child_len: Leaf::Length,
        ) -> Option<Self> {
            debug_assert!(at_offset <= self.children.len());

            if self.is_full() {
                let split_offset = self.children.len() - Self::min_children();

                // Split so that the extra inode always has the minimum number
                // of children.
                let rest = if at_offset <= Self::min_children() {
                    let rest = self.split(split_offset);
                    self.insert(at_offset, child, child_len);
                    rest
                } else {
                    let mut rest = self.split(split_offset + 1);
                    rest.insert(
                        at_offset - self.children.len(),
                        child,
                        child_len,
                    );
                    rest
                };

                debug_assert_eq!(rest.children.len(), Self::min_children());

                Some(rest)
            } else {
                self.summary += child_len;
                // SAFETY: Self is not full.
                unsafe { self.children.insert(at_offset, (child_len, child)) };
                None
            }
        }

        /// TODO: docs
        pub fn insert_two(
            &mut self,
            mut a_offset: usize,
            mut a: NodePtr<Leaf>,
            mut a_len: Leaf::Length,
            mut b_offset: usize,
            mut b: NodePtr<Leaf>,
            mut b_len: Leaf::Length,
        ) -> Option<Self> {
            use core::cmp::Ordering;

            debug_assert!(Self::min_children() >= 2);

            if a_offset > b_offset {
                (a, b, a_offset, b_offset, a_len, b_len) =
                    (b, a, b_offset, a_offset, b_len, a_len)
            }

            debug_assert!(b_offset <= self.children.len());

            if Self::max_children() - self.children.len() < 2 {
                let split_offset = self.children.len() - Self::min_children();

                let children_after_b = self.children.len() - b_offset;

                // Split so that the extra inode always has the minimum number
                // of children.
                //
                // The logic to make this work is a bit annoying to reason
                // about. We should probably add some unit tests to avoid
                // possible regressions.
                let rest = match children_after_b
                    .cmp(&(Self::min_children() - 1))
                {
                    Ordering::Greater => {
                        let rest = self.split(split_offset);
                        self.insert_two(
                            a_offset, a, a_len, b_offset, b, b_len,
                        );
                        rest
                    },

                    Ordering::Less if a_offset >= split_offset + 2 => {
                        let mut rest = self.split(split_offset + 2);
                        a_offset -= self.children.len();
                        b_offset -= self.children.len();
                        rest.insert_two(
                            a_offset, a, a_len, b_offset, b, b_len,
                        );
                        rest
                    },

                    _ => {
                        let mut rest = self.split(split_offset + 1);
                        rest.insert(b_offset - self.children.len(), b, b_len);
                        self.insert(a_offset, a, a_len);
                        rest
                    },
                };

                debug_assert_eq!(rest.children.len(), Self::min_children());

                Some(rest)
            } else {
                self.insert(a_offset, a, a_len);
                self.insert(b_offset + 1, b, b_len);
                None
            }
        }

        #[inline]
        pub fn insert_leaf(
            &mut self,
            at_offset: usize,
            leaf: Leaf,
        ) -> Option<Self> {
            debug_assert!(self.has_leaves);
            let summary = leaf.summarize();
            self.insert(at_offset, NodePtr::from_leaf(leaf), summary)
        }

        #[inline]
        pub fn insert_idx(
            &mut self,
            at_offset: usize,
            idx: GtreeIdx,
            len: Leaf::Length,
        ) -> Option<Self> {
            debug_assert!(!self.has_leaves);
            self.insert(at_offset, NodePtr::from_idx(idx), len)
        }

        #[inline]
        pub fn insert_two_leaves(
            &mut self,
            first_offset: usize,
            first_leaf: Leaf,
            second_offset: usize,
            second_leaf: Leaf,
        ) -> Option<Self> {
            let first_len = first_leaf.summarize();
            let second_len = first_leaf.summarize();
            self.insert_two(
                first_offset,
                NodePtr::from_leaf(first_leaf),
                first_len,
                second_offset,
                NodePtr::from_leaf(second_leaf),
                second_len,
            )
        }

        #[inline]
        pub fn insert_two_idxs(
            &mut self,
            first_offset: usize,
            first_idx: GtreeIdx,
            first_len: Leaf::Length,
            second_offset: usize,
            second_idx: GtreeIdx,
            second_len: Leaf::Length,
        ) -> Option<Self> {
            self.insert_two(
                first_offset,
                NodePtr::from_idx(first_idx),
                first_len,
                second_offset,
                NodePtr::from_idx(second_idx),
                second_len,
            )
        }

        #[inline]
        fn is_full(&self) -> bool {
            self.children.is_full()
        }

        #[inline]
        pub fn len(&self) -> Leaf::Length {
            self.summary
        }

        #[inline]
        const fn max_children() -> usize {
            ARITY
        }

        #[inline]
        const fn min_children() -> usize {
            ARITY / 2
        }

        #[inline]
        pub fn new_root(
            (old_root, old_summary): (GtreeIdx, Leaf::Length),
            (root_split, split_summary): (GtreeIdx, Leaf::Length),
        ) -> Self {
            let mut children = Children::new();

            let total_summary = old_summary + split_summary;

            // SAFETY: the children are still empty so definitely not full.
            unsafe {
                children.push((old_summary, NodePtr::from_idx(old_root)));
                children.push((split_summary, NodePtr::from_idx(root_split)));
            };

            Self {
                summary: total_summary,
                parent: GtreeIdx::dangling(),
                has_leaves: false,
                children,
            }
        }

        #[inline]
        pub fn parent_mut(&mut self) -> &mut GtreeIdx {
            &mut self.parent
        }

        #[inline]
        fn split(&mut self, at_offset: usize) -> Self {
            let other_children = unsafe { self.children.split(at_offset) };

            let (new_summary, other_summary) =
                if self.children.len() < other_children.len() {
                    let mut s = Leaf::Length::default();
                    for &(child_summary, _) in self.children.as_slice() {
                        s += child_summary;
                    }
                    (s, self.summary - s)
                } else {
                    let mut s = Leaf::Length::default();
                    for &(child_summary, _) in other_children.as_slice() {
                        s += child_summary;
                    }
                    (self.summary - s, s)
                };

            self.summary = new_summary;

            Self {
                parent: GtreeIdx::dangling(),
                has_leaves: self.has_leaves,
                summary: other_summary,
                children: other_children,
            }
        }
    }

    impl<const ARITY: usize, Leaf: Summarize + Debug> Debug
        for Inode<ARITY, Leaf>
    {
        fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
            struct DebugInternal<Leaf: Summarize> {
                len: Leaf::Length,
                idx: GtreeIdx,
            }

            impl<Leaf: Summarize> Debug for DebugInternal<Leaf> {
                fn fmt(
                    &self,
                    f: &mut core::fmt::Formatter,
                ) -> core::fmt::Result {
                    write!(f, "{:?} @ {:?}", self.len, self.idx)
                }
            }

            let children = self.children();

            if !self.parent.is_dangling() {
                write!(f, "{:?} <- ", self.parent)?;
            }

            write!(f, "{:?} @ ", self.summary)?;

            let mut dbg = f.debug_list();

            if self.has_leaves {
                for (_, child) in children {
                    let leaf = unsafe { child.as_leaf() };
                    dbg.entry(leaf);
                }
            } else {
                for &(len, ref child) in children {
                    let idx = unsafe { child.as_idx() };
                    let internal = DebugInternal::<Leaf> { len, idx };
                    dbg.entry(&internal);
                }
            }

            dbg.finish()
        }
    }

    impl<const ARITY: usize, Leaf: Summarize + Clone> Clone
        for Inode<ARITY, Leaf>
    {
        #[inline]
        fn clone(&self) -> Self {
            let mut new_children = Children::new();

            if self.has_leaves {
                for &(len, ref child) in self.children.as_slice() {
                    // SAFETY: this inode stores leaves so the child points to
                    // a leaf that's never been dropped.
                    let leaf = unsafe { child.as_leaf() };
                    let ptr = NodePtr::from_leaf(leaf.clone());
                    unsafe { new_children.push((len, ptr)) };
                }
            } else {
                for &(len, ref child) in self.children.as_slice() {
                    // SAFETY: this inode stores leaves so the child points to
                    // a leaf that's never been dropped.
                    let idx = unsafe { child.as_idx() };
                    let ptr = NodePtr::from_idx(idx);
                    unsafe { new_children.push((len, ptr)) };
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
                for (_, child) in self.children.as_mut_slice() {
                    // SAFETY: this inode stores leaves so the child points to
                    // a leaf that's never been dropped.
                    unsafe {
                        core::ptr::drop_in_place(child.as_leaf_mut());
                    }
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

            split.len = self.len - at_offset;

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
        pub fn from_idx(tree_idx: GtreeIdx) -> Self {
            Self { to_internal: tree_idx }
        }

        #[inline]
        pub fn from_leaf(leaf: Leaf) -> Self {
            let ptr = Box::leak::<'static>(Box::new(leaf));
            Self { to_leaf: unsafe { NonNull::new_unchecked(ptr) } }
        }
    }
}

mod insert {
    //! TODO: docs.

    use super::*;

    pub(super) fn insert_at_offset<const N: usize, L, F>(
        gtree: &mut Gtree<N, L>,
        idx: GtreeIdx,
        at_offset: L::Length,
        fun: F,
    ) -> Option<Inode<N, L>>
    where
        L: Summarize,
        F: FnOnce(&mut L, L::Length) -> (Option<L>, Option<L>),
    {
        let inode = gtree.inode(idx);

        let (child_idx, offset) = inode.child_at_offset::<false>(at_offset);

        match gtree.inode_mut(idx).child_mut(child_idx) {
            Either::Left(next_idx) => {
                let maybe_split =
                    insert_at_offset(gtree, next_idx, at_offset - offset, fun);

                let new_summary = gtree.inode(next_idx).len();

                let inode = gtree.inode_mut(idx);

                inode.update_summary(child_idx, new_summary);

                if let Some(split) = maybe_split {
                    let len = split.len();
                    let pushed_idx = gtree.push(split, idx);
                    let inode = gtree.inode_mut(idx);
                    inode.insert_idx(child_idx + 1, pushed_idx, len)
                } else {
                    None
                }
            },

            Either::Right(leaf) => {
                let (left, right) = fun(leaf, offset);

                let new_summary = leaf.summarize();

                let inode = gtree.inode_mut(idx);

                inode.update_summary(child_idx, new_summary);

                if let Some(left) = left {
                    let offset = child_idx + 1;

                    if let Some(right) = right {
                        inode.insert_two_leaves(offset, left, offset, right)
                    } else {
                        inode.insert_leaf(offset, left)
                    }
                } else {
                    None
                }
            },
        }
    }
}

mod delete {
    //! TODO: docs.

    use super::*;

    pub(super) fn delete_range<
        const N: usize,
        L,
        DelLeaf,
        DelFrom,
        DelUpTo,
        DelRange,
    >(
        gtree: &mut Gtree<N, L>,
        idx: GtreeIdx,
        mut range: Range<L::Length>,
        del_leaf: &mut DelLeaf,
        del_from: DelFrom,
        del_up_to: DelUpTo,
        del_range: DelRange,
    ) -> Option<Inode<N, L>>
    where
        L: Summarize,
        DelLeaf: FnMut(&mut L),
        DelFrom: FnOnce(&mut L, L::Length) -> Option<L>,
        DelUpTo: FnOnce(&mut L, L::Length) -> Option<L>,
        DelRange: FnOnce(&mut L, Range<L::Length>) -> (Option<L>, Option<L>),
    {
        let inode = gtree.inode(idx);

        let Some((child_idx, offset)) =
            inode.child_containing_range(range.clone()) else {
                return delete_range_in_inode(
                    gtree,
                    idx,
                    range,
                    del_leaf,
                    del_from,
                    del_up_to,
                );
            };

        range.start -= offset;

        range.end -= offset;

        match gtree.inode_mut(idx).child_mut(child_idx) {
            Either::Left(next_idx) => {
                let maybe_split = delete_range(
                    gtree, next_idx, range, del_leaf, del_from, del_up_to,
                    del_range,
                );

                let new_summary = gtree.inode(next_idx).len();

                let inode = gtree.inode_mut(idx);

                inode.update_summary(child_idx, new_summary);

                if let Some(split) = maybe_split {
                    let len = split.len();
                    let pushed_idx = gtree.push(split, idx);
                    let inode = gtree.inode_mut(idx);
                    inode.insert_idx(child_idx + 1, pushed_idx, len)
                } else {
                    None
                }
            },

            Either::Right(leaf) => {
                let (left, right) = del_range(leaf, range);

                let new_summary = leaf.summarize();

                let inode = gtree.inode_mut(idx);

                inode.update_summary(child_idx, new_summary);

                if let Some(left) = left {
                    let offset = child_idx + 1;

                    if let Some(right) = right {
                        inode.insert_two_leaves(offset, left, offset, right)
                    } else {
                        inode.insert_leaf(offset, left)
                    }
                } else {
                    None
                }
            },
        }
    }

    fn delete_range_in_inode<const N: usize, L, DelLeaf, DelFrom, DelUpTo>(
        gtree: &mut Gtree<N, L>,
        idx: GtreeIdx,
        range: Range<L::Length>,
        del_leaf: &mut DelLeaf,
        del_from: DelFrom,
        del_up_to: DelUpTo,
    ) -> Option<Inode<N, L>>
    where
        L: Summarize,
        DelLeaf: FnMut(&mut L),
        DelFrom: FnOnce(&mut L, L::Length) -> Option<L>,
        DelUpTo: FnOnce(&mut L, L::Length) -> Option<L>,
    {
        let inode = gtree.inode_mut(idx);

        let (start_idx, offset) = inode.child_at_offset::<true>(range.start);

        let delete_from_len = range.start - offset;

        let extra_from_start = match inode.child_mut(start_idx) {
            Either::Left(child_idx) => {
                let maybe_split = delete_from(
                    gtree,
                    child_idx,
                    delete_from_len,
                    del_leaf,
                    del_from,
                );

                let new_summary = gtree.inode(child_idx).len();
                let inode = gtree.inode_mut(idx);
                inode.update_summary(start_idx, new_summary);

                maybe_split.map(|inode| {
                    let len = inode.len();
                    let idx = gtree.push(inode, idx);
                    (NodePtr::from_idx(idx), len)
                })
            },

            Either::Right(leaf) => {
                let extra = del_from(leaf, delete_from_len);

                let new_summary = leaf.summarize();
                let inode = gtree.inode_mut(idx);
                inode.update_summary(start_idx, new_summary);

                extra.map(|leaf| {
                    let len = leaf.summarize();
                    (NodePtr::from_leaf(leaf), len)
                })
            },
        };

        let inode = gtree.inode_mut(idx);

        let (end_idx, offset) = inode.child_at_offset::<false>(range.end);

        inode.delete_children(start_idx + 1..end_idx, del_leaf);

        let delete_up_to_len = range.end - offset;

        let extra_from_end = match inode.child_mut(end_idx) {
            Either::Left(child_idx) => {
                let maybe_split = delete_up_to(
                    gtree,
                    child_idx,
                    delete_up_to_len,
                    del_leaf,
                    del_up_to,
                );

                let new_summary = gtree.inode(child_idx).len();
                let inode = gtree.inode_mut(idx);
                inode.update_summary(start_idx, new_summary);

                maybe_split.map(|inode| {
                    let len = inode.len();
                    let idx = gtree.push(inode, idx);
                    (NodePtr::from_idx(idx), len)
                })
            },

            Either::Right(leaf) => {
                let split = del_up_to(leaf, delete_up_to_len);

                let new_summary = leaf.summarize();
                let inode = gtree.inode_mut(idx);
                inode.update_summary(end_idx, new_summary);

                split.map(|leaf| {
                    let len = leaf.summarize();
                    (NodePtr::from_leaf(leaf), len)
                })
            },
        };

        let start_offset = start_idx + 1;

        let end_offset = start_idx + 1;

        match (extra_from_start, extra_from_end) {
            (Some((start, start_len)), Some((end, end_len))) => {
                let inode = gtree.inode_mut(idx);
                inode.insert_two(
                    start_offset,
                    start,
                    start_len,
                    end_offset,
                    end,
                    end_len,
                )
            },

            (Some((start, len)), None) => {
                let inode = gtree.inode_mut(idx);
                inode.insert(start_offset, start, len)
            },

            (None, Some((end, len))) => {
                let inode = gtree.inode_mut(idx);
                inode.insert(end_offset, end, len)
            },

            (None, None) => None,
        }
    }

    fn delete_from<const N: usize, L, DelLeaf, DelFrom>(
        gtree: &mut Gtree<N, L>,
        idx: GtreeIdx,
        from: L::Length,
        del_leaf: &mut DelLeaf,
        del_from: DelFrom,
    ) -> Option<Inode<N, L>>
    where
        L: Summarize,
        DelLeaf: FnMut(&mut L),
        DelFrom: FnOnce(&mut L, L::Length) -> Option<L>,
    {
        let inode = gtree.inode(idx);

        let (child_idx, offset) = inode.child_at_offset::<true>(from);

        let from = from - offset;

        let split = match gtree.inode_mut(idx).child_mut(child_idx) {
            Either::Left(next_idx) => {
                let maybe_split =
                    delete_from(gtree, idx, from, del_leaf, del_from);

                let new_summary = gtree.inode(next_idx).len();

                let inode = gtree.inode_mut(idx);

                inode.update_summary(child_idx, new_summary);

                if let Some(split) = maybe_split {
                    let len = split.len();
                    let pushed_idx = gtree.push(split, idx);
                    let inode = gtree.inode_mut(idx);
                    inode.insert_idx(child_idx + 1, pushed_idx, len)
                } else {
                    None
                }
            },

            Either::Right(leaf) => {
                let split = del_from(leaf, from);

                let new_summary = leaf.summarize();

                let inode = gtree.inode_mut(idx);

                inode.update_summary(child_idx, new_summary);

                if let Some(left) = split {
                    let offset = child_idx + 1;
                    inode.insert_leaf(offset, left)
                } else {
                    None
                }
            },
        };

        let inode = gtree.inode_mut(idx);

        let len = inode.children().len();

        inode.delete_children(child_idx + 1..len, del_leaf);

        split
    }

    fn delete_up_to<const N: usize, L, DelLeaf, DelUpTo>(
        gtree: &mut Gtree<N, L>,
        idx: GtreeIdx,
        up_to: L::Length,
        del_leaf: &mut DelLeaf,
        del_up_to: DelUpTo,
    ) -> Option<Inode<N, L>>
    where
        L: Summarize,
        DelLeaf: FnMut(&mut L),
        DelUpTo: FnOnce(&mut L, L::Length) -> Option<L>,
    {
        let inode = gtree.inode_mut(idx);

        let (child_idx, offset) = inode.child_at_offset::<false>(up_to);

        inode.delete_children(0..child_idx, del_leaf);

        let up_to = up_to - offset;

        let split = match gtree.inode_mut(idx).child_mut(child_idx) {
            Either::Left(next_idx) => {
                let maybe_split =
                    delete_up_to(gtree, next_idx, up_to, del_leaf, del_up_to);

                let new_summary = gtree.inode(next_idx).len();
                let inode = gtree.inode_mut(idx);
                inode.update_summary(child_idx, new_summary);

                if let Some(split) = maybe_split {
                    let len = split.len();
                    let pushed_idx = gtree.push(split, idx);
                    let inode = gtree.inode_mut(idx);
                    inode.insert_idx(child_idx + 1, pushed_idx, len)
                } else {
                    None
                }
            },

            Either::Right(leaf) => {
                let split = del_up_to(leaf, up_to);

                let new_summary = leaf.summarize();

                let inode = gtree.inode_mut(idx);

                inode.update_summary(child_idx, new_summary);

                if let Some(left) = split {
                    let offset = child_idx + 1;
                    inode.insert_leaf(offset, left)
                } else {
                    None
                }
            },
        };

        split
    }
}
