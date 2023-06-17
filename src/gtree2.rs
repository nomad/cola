use core::fmt::Debug;
use core::ops::{Add, AddAssign, Range, Sub, SubAssign};

/// TODO: docs
pub trait Summary:
    Debug
    + Clone
    + Add<Self, Output = Self>
    + AddAssign<Self>
    + Sub<Self, Output = Self>
    + SubAssign<Self>
{
    fn empty() -> Self;
}

/// TODO: docs
pub trait Summarize: Debug {
    type Summary: Summary;

    fn summarize(&self) -> Self::Summary;
}

/// TODO: docs
pub trait Metric<S: Summary>:
    Debug
    + Copy
    + Add<Self, Output = Self>
    + AddAssign<Self>
    + Sub<Self, Output = Self>
    + SubAssign<Self>
    + Ord
{
    fn zero() -> Self;

    fn measure(summary: &S) -> Self;
}

//pub trait DeletableLeaf: Summarize {
//    type DeletePatch;
//
//    type DeleteInfo: Debug + Default + AddAssign<Self::DeletePatch>;
//
//    fn delete_whole(&mut self) -> DeletePatch;
//
//    fn delete_range(&mut self, range: RangeB<Self::Offset>) -> DeletePatch
//        where
//}

use gtree::InodeIdx;
pub use gtree::{Gtree, LeafIdx};

mod gtree {
    use super::*;

    /// TODO: docs
    #[derive(Clone)]
    pub struct Gtree<const ARITY: usize, Leaf: Summarize> {
        /// TODO: docs
        inodes: Vec<Inode<ARITY, Leaf>>,

        /// TODO: docs
        lnodes: Vec<Lnode<Leaf>>,

        /// TODO: docs
        root_idx: InodeIdx,
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    pub(super) struct InodeIdx(usize);

    impl Debug for InodeIdx {
        fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
            write!(f, "InodeIdx({})", self.0)
        }
    }

    impl InodeIdx {
        #[inline]
        pub const fn dangling() -> Self {
            Self(usize::MAX)
        }

        #[inline]
        pub fn is_dangling(self) -> bool {
            self == Self::dangling()
        }
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct LeafIdx(usize);

    impl Debug for LeafIdx {
        fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
            write!(f, "LeafIdx({})", self.0)
        }
    }

    impl<const ARITY: usize, Leaf: Summarize> Gtree<ARITY, Leaf> {
        #[doc(hidden)]
        pub fn debug_as_btree(&self) -> DebugAsBtree<'_, ARITY, Leaf> {
            DebugAsBtree(self)
        }

        #[doc(hidden)]
        pub fn debug_as_self(&self) -> DebugAsSelf<'_, ARITY, Leaf> {
            DebugAsSelf(self)
        }

        #[inline]
        pub(super) fn inode(&self, idx: InodeIdx) -> &Inode<ARITY, Leaf> {
            &self.inodes[idx.0]
        }

        #[inline]
        pub(super) fn inode_mut(
            &mut self,
            idx: InodeIdx,
        ) -> &mut Inode<ARITY, Leaf> {
            &mut self.inodes[idx.0]
        }

        /// TODO: docs
        #[inline]
        pub fn insert_at_offset<M: Metric<Leaf::Summary>, F>(
            &mut self,
            offset: M,
            insert_with: F,
        ) -> (LeafIdx, Option<LeafIdx>, Option<LeafIdx>)
        where
            F: FnOnce(&mut Leaf, M) -> (Option<Leaf>, Option<Leaf>),
        {
            let (idxs, maybe_split) = insert::insert_at_offset(
                self,
                self.root_idx,
                offset,
                insert_with,
            );

            if let Some(root_split) = maybe_split {
                self.root_has_split(root_split);
            }

            idxs
        }

        #[inline(always)]
        pub(super) fn leaf(&self, idx: LeafIdx) -> &Leaf {
            self.lnode(idx).value()
        }

        #[inline(always)]
        pub(super) fn leaf_mut(&mut self, idx: LeafIdx) -> &mut Leaf {
            self.lnode_mut(idx).value_mut()
        }

        #[inline]
        pub(super) fn lnode(&self, idx: LeafIdx) -> &Lnode<Leaf> {
            &self.lnodes[idx.0]
        }

        #[inline]
        pub(super) fn lnode_mut(&mut self, idx: LeafIdx) -> &mut Lnode<Leaf> {
            &mut self.lnodes[idx.0]
        }

        /// TODO: docs
        #[inline]
        pub fn new(first_leaf: Leaf) -> Self {
            let summary = first_leaf.summarize();

            let leaf_idx = LeafIdx(0);
            let root_idx = InodeIdx(0);

            let dangling = InodeIdx::dangling();

            let mut inodes = Vec::with_capacity(256);
            let inode = Inode::from_leaf(leaf_idx, summary, dangling);
            inodes.push(inode);

            let mut lnodes = Vec::with_capacity(256);
            let lnode = Lnode::new(first_leaf, root_idx);
            lnodes.push(lnode);

            Self { inodes, lnodes, root_idx }
        }

        /// TODO: docs
        #[inline(always)]
        pub(super) fn push_inode(
            &mut self,
            inode: Inode<ARITY, Leaf>,
        ) -> InodeIdx {
            let idx = InodeIdx(self.inodes.len());
            assert!(!idx.is_dangling());
            Inode::pushed(inode, self, idx);
            idx
        }

        /// TODO: docs
        #[inline]
        pub(super) fn push_leaf(
            &mut self,
            leaf: Leaf,
            parent: InodeIdx,
        ) -> LeafIdx {
            let idx = LeafIdx(self.lnodes.len());
            self.lnodes.push(Lnode::new(leaf, parent));
            idx
        }

        #[inline]
        fn root(&self) -> &Inode<ARITY, Leaf> {
            self.inode(self.root_idx)
        }

        /// TODO: docs
        #[inline]
        fn root_has_split(&mut self, root_split: Inode<ARITY, Leaf>) {
            let new_summary = self.summary() + root_split.summary();

            let split_idx = self.push_inode(root_split);

            let new_root = Inode::from_two_internals(
                self.root_idx,
                split_idx,
                new_summary,
                InodeIdx::dangling(),
            );

            let new_root_idx = self.push_inode(new_root);

            self.root_mut().set_parent(new_root_idx);

            self.inode_mut(split_idx).set_parent(new_root_idx);

            self.root_idx = new_root_idx;
        }

        #[inline]
        fn root_mut(&mut self) -> &mut Inode<ARITY, Leaf> {
            self.inode_mut(self.root_idx)
        }

        #[inline]
        pub fn summary(&self) -> Leaf::Summary {
            self.root().summary()
        }

        /// TODO: docs
        pub(super) fn with_internal_mut<F, R>(
            &mut self,
            inode_idx: InodeIdx,
            idx_in_parent: usize,
            fun: F,
        ) -> (R, Option<Inode<ARITY, Leaf>>)
        where
            F: FnOnce(&mut Self) -> (R, Option<Inode<ARITY, Leaf>>),
        {
            let old_summary = self.inode(inode_idx).summary();

            let (ret, maybe_split) = fun(self);

            let inode = self.inode(inode_idx);

            let new_summary = inode.summary();

            let parent_idx = inode.parent();

            debug_assert!(!parent_idx.is_dangling());

            let parent = self.inode_mut(parent_idx);

            parent.update_child_summary(
                idx_in_parent,
                old_summary,
                new_summary,
            );

            let split = if let Some(split) = maybe_split {
                let summary = split.summary();
                let split_idx = self.push_inode(split);
                let parent = self.inode_mut(parent_idx);
                parent.insert_inode(idx_in_parent + 1, split_idx, summary)
            } else {
                None
            };

            (ret, split)
        }

        /// TODO: docs
        pub(super) fn with_leaf_mut<F>(
            &mut self,
            leaf_idx: LeafIdx,
            idx_in_parent: usize,
            fun: F,
        ) -> (Option<LeafIdx>, Option<LeafIdx>, Option<Inode<ARITY, Leaf>>)
        where
            F: FnOnce(&mut Leaf) -> (Option<Leaf>, Option<Leaf>),
        {
            let lnode = self.lnode_mut(leaf_idx);

            let leaf = lnode.value_mut();

            let old_summary = leaf.summarize();

            let (left, right) = fun(leaf);

            let new_summary = leaf.summarize();

            let parent_idx = lnode.parent();

            let parent = self.inode_mut(parent_idx);

            parent.update_child_summary(
                idx_in_parent,
                old_summary,
                new_summary,
            );

            let insert_at = idx_in_parent + 1;

            match (left, right) {
                (Some(left), Some(right)) => {
                    let summary = left.summarize() + right.summarize();

                    let left_idx = self.push_leaf(left, parent_idx);

                    let right_idx = self.push_leaf(right, parent_idx);

                    let parent = self.inode_mut(parent_idx);

                    let split = parent.insert_two_leaves(
                        insert_at, left_idx, insert_at, right_idx, summary,
                    );

                    (Some(left_idx), Some(right_idx), split)
                },

                (Some(left), None) => {
                    let summary = left.summarize();

                    let left_idx = self.push_leaf(left, parent_idx);

                    let parent = self.inode_mut(parent_idx);

                    let split =
                        parent.insert_leaf(insert_at, left_idx, summary);

                    (Some(left_idx), None, split)
                },

                _ => (None, None, None),
            }
        }
    }

    impl<const ARITY: usize, Leaf: Summarize> Debug for Gtree<ARITY, Leaf> {
        fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
            self.debug_as_self().fmt(f)
        }
    }

    struct DebugAsDisplay<'a>(&'a dyn core::fmt::Display);

    impl Debug for DebugAsDisplay<'_> {
        fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
            core::fmt::Display::fmt(self.0, f)
        }
    }

    pub struct DebugAsSelf<'a, const N: usize, L: Summarize>(&'a Gtree<N, L>);

    impl<const N: usize, L: Summarize> Debug for DebugAsSelf<'_, N, L> {
        fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
            let gtree = self.0;

            let debug_inodes = DebugInodesSequentially {
                inodes: gtree.inodes.as_slice(),
                root_idx: gtree.root_idx.0,
            };

            let debug_lnodes =
                DebugLnodesSequentially(gtree.lnodes.as_slice());

            let mut dbg = f.debug_map();

            dbg.entry(&DebugAsDisplay(&"inodes"), &debug_inodes)
                .entry(&DebugAsDisplay(&"lnodes"), &debug_lnodes)
                .finish()
        }
    }

    struct DebugInodesSequentially<'a, const N: usize, L: Summarize> {
        inodes: &'a [Inode<N, L>],
        root_idx: usize,
    }

    impl<const N: usize, L: Summarize> Debug
        for DebugInodesSequentially<'_, N, L>
    {
        fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
            struct Key {
                idx: usize,
                is_root: bool,
            }

            impl Debug for Key {
                fn fmt(
                    &self,
                    f: &mut core::fmt::Formatter,
                ) -> core::fmt::Result {
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

    struct DebugLnodesSequentially<'a, L: Summarize>(&'a [Lnode<L>]);

    impl<L: Summarize> Debug for DebugLnodesSequentially<'_, L> {
        fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
            f.debug_map()
                .entries(self.0.iter().map(Lnode::value).enumerate())
                .finish()
        }
    }

    pub struct DebugAsBtree<'a, const N: usize, L: Summarize>(&'a Gtree<N, L>);

    impl<const N: usize, L: Summarize> Debug for DebugAsBtree<'_, N, L> {
        fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
            fn print_inode_as_tree<const N: usize, L: Summarize>(
                gtree: &Gtree<N, L>,
                inode_idx: InodeIdx,
                shifts: &mut String,
                ident: &str,
                last_shift_byte_len: usize,
                f: &mut core::fmt::Formatter,
            ) -> core::fmt::Result {
                let inode = gtree.inode(inode_idx);

                writeln!(
                    f,
                    "{}{}{:?}",
                    &shifts[..shifts.len() - last_shift_byte_len],
                    ident,
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
                            let ident = ident(i);
                            let leaf = gtree.leaf(leaf_idx);
                            writeln!(f, "{}{}{:#?}", &shifts, ident, &leaf)?;
                        }
                    },
                }

                Ok(())
            }

            print_inode_as_tree(
                self.0,
                self.0.root_idx,
                &mut String::new(),
                "",
                0,
                f,
            )
        }
    }
}

use inode::{Either, Inode};

mod inode {
    use super::*;

    /// TODO: docs
    #[derive(Clone)]
    pub(super) struct Inode<const ARITY: usize, Leaf: Summarize> {
        /// TODO: docs
        summary: Leaf::Summary,

        /// TODO: docs
        parent: InodeIdx,

        /// TODO: docs
        has_leaves: bool,

        /// TODO: docs
        len: usize,

        /// TODO: docs
        children: [NodeIdx; ARITY],
    }

    /// TODO: docs
    #[derive(Clone, Copy)]
    union NodeIdx {
        internal: InodeIdx,
        leaf: LeafIdx,
    }

    impl NodeIdx {
        #[inline]
        const fn dangling() -> Self {
            Self { internal: InodeIdx::dangling() }
        }

        #[inline]
        const fn new_internal(internal_idx: InodeIdx) -> Self {
            Self { internal: internal_idx }
        }

        #[inline]
        const fn new_leaf(leaf_idx: LeafIdx) -> Self {
            Self { leaf: leaf_idx }
        }
    }

    impl<const ARITY: usize, Leaf: Summarize> Inode<ARITY, Leaf> {
        /// TODO: docs
        #[inline]
        pub fn child(&self, child_idx: usize) -> Either<InodeIdx, LeafIdx> {
            let child = self.children[child_idx];

            if self.has_leaves {
                Either::Leaf(unsafe { child.leaf })
            } else {
                Either::Internal(unsafe { child.internal })
            }
        }

        /// TODO: docs
        #[inline]
        pub fn child_at_offset<const WITH_RIGHT_BIAS: bool, M>(
            &self,
            offset: M,
        ) -> (usize, M)
        where
            M: Metric<Leaf::Summary>,
        {
            todo!()
        }

        /// TODO: docs
        #[inline]
        pub fn child_containing_range(
            &self,
            range: Range<Leaf::Summary>,
        ) -> Option<(usize, Leaf::Summary)> {
            todo!();

            //let mut offset = Leaf::Summary::default();

            //let children = self.children();

            //for (idx, &(child_len, _)) in children.iter().enumerate() {
            //    offset += child_len;

            //    if offset > range.start {
            //        if offset >= range.end {
            //            return Some((idx, offset - child_len));
            //        } else {
            //            return None;
            //        }
            //    }
            //}

            unreachable!();
        }

        /// TODO: docs
        #[inline]
        pub fn children(&self) -> Either<&[InodeIdx], &[LeafIdx]> {
            use core::mem::transmute;

            let children = &self.children[..self.len];

            // SAFETY: `LeafIdx` and `InodeIdx` have the same layout, so the
            // `NodeIdx` union also has the same layout and we can safely
            // transmute it into either of them.

            if self.has_leaves {
                let leaves = unsafe { transmute::<_, &[LeafIdx]>(children) };
                Either::Leaf(leaves)
            } else {
                let inodes = unsafe { transmute::<_, &[InodeIdx]>(children) };
                Either::Internal(inodes)
            }
        }

        /// TODO: docs
        #[inline]
        pub fn from_leaf(
            leaf: LeafIdx,
            summary: Leaf::Summary,
            parent: InodeIdx,
        ) -> Self {
            let mut children = [NodeIdx::dangling(); ARITY];

            children[0] = NodeIdx::new_leaf(leaf);

            Self { children, parent, summary, has_leaves: true, len: 1 }
        }

        /// TODO: docs
        #[inline]
        pub fn from_two_internals(
            first: InodeIdx,
            second: InodeIdx,
            total_summary: Leaf::Summary,
            parent: InodeIdx,
        ) -> Self {
            let mut children = [NodeIdx::dangling(); ARITY];

            children[0] = NodeIdx::new_internal(first);

            children[1] = NodeIdx::new_internal(second);

            Self {
                children,
                parent,
                summary: total_summary,
                has_leaves: false,
                len: 2,
            }
        }

        #[inline]
        pub fn insert_inode(
            &mut self,
            at_offset: usize,
            inode_idx: InodeIdx,
            inode_summary: Leaf::Summary,
        ) -> Option<Self> {
            todo!();
        }

        #[inline]
        pub fn insert_leaf(
            &mut self,
            at_offset: usize,
            leaf_idx: LeafIdx,
            leaf_summary: Leaf::Summary,
        ) -> Option<Self> {
            todo!();
        }

        #[inline]
        pub fn insert_two_leaves(
            &mut self,
            first_offset: usize,
            first_leaf: LeafIdx,
            second_offset: usize,
            second_leaf: LeafIdx,
            combined_summary: Leaf::Summary,
        ) -> Option<Self> {
            todo!();
        }

        #[inline(always)]
        pub(super) fn len(&self) -> usize {
            self.len
        }

        #[inline(always)]
        pub(super) fn parent(&self) -> InodeIdx {
            self.parent
        }

        /// TODO: docs
        #[inline]
        pub(super) fn pushed(
            self,
            gtree: &mut Gtree<ARITY, Leaf>,
            to_idx: InodeIdx,
        ) {
            match self.children() {
                Either::Internal(internal_idxs) => {
                    for &idx in internal_idxs {
                        let child_inode = gtree.inode_mut(idx);
                        child_inode.set_parent(to_idx);
                    }
                },

                Either::Leaf(leaf_idxs) => {
                    for &idx in leaf_idxs {
                        let child_lnode = gtree.lnode_mut(idx);
                        child_lnode.set_parent(to_idx);
                    }
                },
            }
        }

        #[inline(always)]
        pub fn set_parent(&mut self, new_parent: InodeIdx) {
            self.parent = new_parent;
        }

        #[inline]
        pub fn summary(&self) -> Leaf::Summary {
            self.summary.clone()
        }

        #[inline]
        pub fn update_child_summary(
            &mut self,
            child_idx: usize,
            old_summary: Leaf::Summary,
            new_summary: Leaf::Summary,
        ) {
            self.summary -= old_summary;
            self.summary += new_summary;
        }
    }

    impl<const ARITY: usize, Leaf: Summarize> Debug for Inode<ARITY, Leaf> {
        fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
            if !self.parent.is_dangling() {
                write!(f, "{:?} <- ", self.parent)?;
            }

            write!(f, "{:?} @ ", self.summary)?;

            let mut dbg = f.debug_list();

            match self.children() {
                Either::Internal(inode_idxs) => {
                    dbg.entries(inode_idxs).finish()
                },
                Either::Leaf(leaf_idxs) => dbg.entries(leaf_idxs).finish(),
            }
        }
    }
    pub(super) enum Either<I, L> {
        Internal(I),
        Leaf(L),
    }
}

use lnode::Lnode;

mod lnode {
    use super::*;

    /// TODO: docs
    #[derive(Clone)]
    pub struct Lnode<Leaf> {
        /// TODO: docs
        value: Leaf,

        /// TODO: docs
        parent: InodeIdx,
    }

    impl<Leaf: Summarize> Lnode<Leaf> {
        #[inline(always)]
        pub(super) fn new(value: Leaf, parent: InodeIdx) -> Self {
            Self { value, parent }
        }

        #[inline(always)]
        pub(super) fn parent(&self) -> InodeIdx {
            self.parent
        }

        #[inline(always)]
        pub(super) fn set_parent(&mut self, new_parent: InodeIdx) {
            self.parent = new_parent;
        }

        #[inline(always)]
        pub(super) fn value(&self) -> &Leaf {
            &self.value
        }

        #[inline(always)]
        pub(super) fn value_mut(&mut self) -> &mut Leaf {
            &mut self.value
        }
    }
}

mod insert {
    //! TODO: docs.

    use super::*;

    pub(super) fn insert_at_offset<const N: usize, L, F, M>(
        gtree: &mut Gtree<N, L>,
        in_inode: InodeIdx,
        mut at_offset: M,
        insert_with: F,
    ) -> ((LeafIdx, Option<LeafIdx>, Option<LeafIdx>), Option<Inode<N, L>>)
    where
        L: Summarize,
        M: Metric<L::Summary>,
        F: FnOnce(&mut L, M) -> (Option<L>, Option<L>),
    {
        let inode = gtree.inode(in_inode);

        let (child_idx, offset) = inode.child_at_offset::<false, _>(at_offset);

        at_offset -= offset;

        match inode.child(child_idx) {
            Either::Internal(next_idx) => {
                gtree.with_internal_mut(next_idx, child_idx, |gtree| {
                    insert_at_offset(gtree, next_idx, at_offset, insert_with)
                })
            },

            Either::Leaf(leaf_idx) => {
                let (first_idx, second_idx, split) =
                    gtree.with_leaf_mut(leaf_idx, child_idx, |leaf| {
                        insert_with(leaf, at_offset)
                    });

                ((leaf_idx, first_idx, second_idx), split)
            },
        }
    }
}

mod delete {
    //! TODO: docs.

    use super::*;

    pub(super) fn delete_range<const N: usize, L, DelRange, DelFrom, DelUpTo>(
        gtree: &mut Gtree<N, L>,
        in_inode: InodeIdx,
        mut range: Range<L::Summary>,
        del_range: DelRange,
        del_from: DelFrom,
        del_up_to: DelUpTo,
    ) -> ((LeafIdx, Option<LeafIdx>, Option<LeafIdx>), Option<Inode<N, L>>)
    where
        L: Summarize,
        DelRange: FnOnce(&mut L, Range<L::Summary>) -> (Option<L>, Option<L>),
        DelFrom: FnOnce(&mut L, L::Summary) -> Option<L>,
        DelUpTo: FnOnce(&mut L, L::Summary) -> Option<L>,
    {
        let inode = gtree.inode(in_inode);

        let Some((child_idx, offset)) =
            inode.child_containing_range(range.clone()) else {
                return delete_range_in_inode(
                    gtree,
                    in_inode,
                    range,
                    del_from,
                    del_up_to,
                );
            };

        todo!();

        range.start -= offset;

        range.end -= offset;

        match inode.child(child_idx) {
            Either::Internal(next_idx) => {
                gtree.with_internal_mut(next_idx, child_idx, |gtree| {
                    delete_range(
                        gtree, next_idx, range, del_range, del_from, del_up_to,
                    )
                })
            },

            Either::Leaf(leaf_idx) => {
                let (first_idx, second_idx, split) =
                    gtree.with_leaf_mut(leaf_idx, child_idx, |leaf| {
                        del_range(leaf, range)
                    });

                ((leaf_idx, first_idx, second_idx), split)
            },
        }
    }

    fn delete_range_in_inode<const N: usize, L, DelFrom, DelUpTo>(
        gtree: &mut Gtree<N, L>,
        idx: InodeIdx,
        range: Range<L::Summary>,
        del_from: DelFrom,
        del_up_to: DelUpTo,
    ) -> ((LeafIdx, Option<LeafIdx>, Option<LeafIdx>), Option<Inode<N, L>>)
    where
        L: Summarize,
        DelFrom: FnOnce(&mut L, L::Summary) -> Option<L>,
        DelUpTo: FnOnce(&mut L, L::Summary) -> Option<L>,
    {
        todo!();
    }

    fn delete_from<const N: usize, L, DelFrom>(
        gtree: &mut Gtree<N, L>,
        idx: InodeIdx,
        mut from: L::Summary,
        del_from: DelFrom,
    ) -> ((LeafIdx, Option<LeafIdx>), Option<Inode<N, L>>)
    where
        L: Summarize,
        DelFrom: FnOnce(&mut L, L::Summary) -> Option<L>,
    {
        let inode = gtree.inode(idx);

        let (child_idx, offset) = todo!();
        //let (child_idx, offset) = inode.child_at_offset::<true, _>(from);

        // TODO: delete children
        // let len = inode.children().len();
        // inode.delete_children(child_idx + 1..len, del_leaf);

        from -= offset;

        match inode.child(child_idx) {
            Either::Internal(next_idx) => {
                gtree.with_internal_mut(next_idx, child_idx, |gtree| {
                    delete_from(gtree, next_idx, from, del_from)
                })
            },

            Either::Leaf(leaf_idx) => {
                let (idx, _none, split) =
                    gtree.with_leaf_mut(leaf_idx, child_idx, |leaf| {
                        (del_from(leaf, from), None)
                    });

                ((leaf_idx, idx), split)
            },
        }
    }

    fn delete_up_to<const N: usize, L, DelUpTo>(
        gtree: &mut Gtree<N, L>,
        idx: InodeIdx,
        mut up_to: L::Summary,
        del_up_to: DelUpTo,
    ) -> ((LeafIdx, Option<LeafIdx>), Option<Inode<N, L>>)
    where
        L: Summarize,
        DelUpTo: FnOnce(&mut L, L::Summary) -> Option<L>,
    {
        let inode = gtree.inode(idx);

        let (child_idx, offset) = todo!();
        // let (child_idx, offset) = inode.child_at_offset::<false>(up_to);

        // TODO: delete children
        // inode.delete_children(0..child_idx, del_leaf);

        up_to -= offset;

        match inode.child(child_idx) {
            Either::Internal(next_idx) => {
                gtree.with_internal_mut(next_idx, child_idx, |gtree| {
                    delete_up_to(gtree, next_idx, up_to, del_up_to)
                })
            },

            Either::Leaf(leaf_idx) => {
                let (idx, _none, split) =
                    gtree.with_leaf_mut(leaf_idx, child_idx, |leaf| {
                        (del_up_to(leaf, up_to), None)
                    });

                ((leaf_idx, idx), split)
            },
        }
    }
}
