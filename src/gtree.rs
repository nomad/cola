use core::fmt::Debug;
use core::ops::{Add, AddAssign, Range, Sub, SubAssign};

/// TODO: docs
pub trait Summary:
    Debug
    + Copy
    + Add<Self, Output = Self>
    + AddAssign<Self>
    + Sub<Self, Output = Self>
    + SubAssign<Self>
    + PartialEq<Self>
{
    type Patch: Copy;

    fn empty() -> Self;

    fn patch(old_summary: Self, new_summary: Self) -> Self::Patch;

    fn apply_patch(&mut self, patch: Self::Patch);

    fn is_empty(&self) -> bool {
        self == &Self::empty()
    }
}

/// TODO: docs
pub trait Summarize: Debug + Clone {
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

        /// TODO: docs
        pub last_insertion_cache: Option<(LeafIdx, Leaf::Summary)>,
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
            self.debug_inode_as_btree(self.root_idx)
        }

        #[doc(hidden)]
        pub fn debug_as_self(&self) -> DebugAsSelf<'_, ARITY, Leaf> {
            DebugAsSelf(self)
        }

        #[doc(hidden)]
        pub(super) fn debug_inode_as_btree(
            &self,
            inode_idx: InodeIdx,
        ) -> DebugAsBtree<'_, ARITY, Leaf> {
            DebugAsBtree { gtree: self, inode_idx }
        }

        /// TODO: docs
        #[inline]
        pub fn delete_range<
            M: Metric<Leaf::Summary>,
            DelRange,
            DelFrom,
            DelUpTo,
        >(
            &mut self,
            range: Range<M>,
            delete_range: DelRange,
            delete_from: DelFrom,
            delete_up_to: DelUpTo,
        ) -> (Option<LeafIdx>, Option<LeafIdx>)
        where
            DelRange:
                FnOnce(&mut Leaf, Range<M>) -> (Option<Leaf>, Option<Leaf>),
            DelFrom: FnOnce(&mut Leaf, M) -> Option<Leaf>,
            DelUpTo: FnOnce(&mut Leaf, M) -> Option<Leaf>,
        {
            self.last_insertion_cache = None;

            let (idxs, maybe_split) = delete::delete_range(
                self,
                self.root_idx,
                range,
                delete_range,
                delete_from,
                delete_up_to,
            );

            if let Some(root_split) = maybe_split {
                self.root_has_split(root_split);
            }

            idxs
        }

        #[inline(always)]
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
        ) -> (Option<LeafIdx>, Option<LeafIdx>)
        where
            F: FnOnce(&mut Leaf, M) -> (Option<Leaf>, Option<Leaf>),
        {
            if let Some((leaf_idx, leaf_offset)) = self.last_insertion_cache {
                let m_offset = M::measure(&leaf_offset);

                let leaf_measure =
                    M::measure(&self.leaf(leaf_idx).summarize());

                if offset > m_offset && offset <= m_offset + leaf_measure {
                    return self.insert_at_leaf(
                        leaf_idx,
                        leaf_offset,
                        offset,
                        insert_with,
                    );
                }
            }

            let (idxs, maybe_split) = insert::insert_at_offset(
                self,
                self.root_idx,
                Leaf::Summary::empty(),
                offset,
                insert_with,
            );

            if let Some(root_split) = maybe_split {
                self.root_has_split(root_split);
            }

            idxs
        }

        fn insert_at_leaf<M, F>(
            &mut self,
            leaf_idx: LeafIdx,
            leaf_offset: Leaf::Summary,
            provided_offset: M,
            insert_with: F,
        ) -> (Option<LeafIdx>, Option<LeafIdx>)
        where
            M: Metric<Leaf::Summary>,
            F: FnOnce(&mut Leaf, M) -> (Option<Leaf>, Option<Leaf>),
        {
            let lnode = self.lnode_mut(leaf_idx);

            let old_leaf = lnode.value().clone();

            let old_summary = old_leaf.summarize();

            let (first, second) = insert_with(
                lnode.value_mut(),
                provided_offset - M::measure(&leaf_offset),
            );

            if first.is_some() {
                let new_leaf = core::mem::replace(lnode.value_mut(), old_leaf);

                self.last_insertion_cache = None;

                return self.insert_at_offset(
                    provided_offset,
                    |old_leaf, _| {
                        *old_leaf = new_leaf;
                        (first, second)
                    },
                );
            }

            let new_summary = lnode.value().summarize();

            let summary_patch = Leaf::Summary::patch(old_summary, new_summary);

            let mut parent = lnode.parent();

            while !parent.is_dangling() {
                let inode = self.inode_mut(parent);
                inode.update_summary(summary_patch);
                parent = inode.parent();
            }

            (None, None)
        }

        #[inline]
        fn is_inode_deleted(&self, inode_idx: InodeIdx) -> bool {
            false
        }

        #[inline]
        fn is_leaf_deleted(&self, leaf_idx: LeafIdx) -> bool {
            false
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

            Self { inodes, lnodes, root_idx, last_insertion_cache: None }
        }

        /// TODO: docs
        #[inline(always)]
        pub(super) fn push_inode(
            &mut self,
            mut inode: Inode<ARITY, Leaf>,
        ) -> InodeIdx {
            let idx = InodeIdx(self.inodes.len());
            assert!(!idx.is_dangling());
            inode.pushed(self, idx);
            self.inodes.push(inode);
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
            let split_summary = root_split.summary();

            let split_idx = self.push_inode(root_split);

            let new_root = Inode::from_two_internals(
                self.root_idx,
                split_idx,
                self.summary(),
                split_summary,
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
        #[inline]
        pub(super) fn with_internal_mut<F, R>(
            &mut self,
            inode_idx: InodeIdx,
            idx_in_parent: usize,
            fun: F,
        ) -> (R, Option<Inode<ARITY, Leaf>>)
        where
            F: FnOnce(&mut Self) -> (R, Option<Inode<ARITY, Leaf>>),
        {
            debug_assert!(inode_idx != self.root_idx);

            let (ret, maybe_split) = fun(self);

            let inode = self.inode(inode_idx);

            let new_summary = inode.summary();

            let parent_idx = inode.parent();

            debug_assert!(!parent_idx.is_dangling());

            let parent = self.inode_mut(parent_idx);

            parent.update_child_summary(idx_in_parent, new_summary);

            let split = if let Some(mut split) = maybe_split {
                split.set_parent(parent_idx);
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
        #[inline]
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

            let (left, right) = fun(leaf);

            let new_summary = leaf.summarize();

            let parent_idx = lnode.parent();

            let parent = self.inode_mut(parent_idx);

            parent.update_child_summary(idx_in_parent, new_summary);

            let insert_at = idx_in_parent + 1;

            match (left, right) {
                (Some(left), Some(right)) => {
                    let left_summary = left.summarize();
                    let right_summary = right.summarize();

                    let left_idx = self.push_leaf(left, parent_idx);

                    let right_idx = self.push_leaf(right, parent_idx);

                    let parent = self.inode_mut(parent_idx);

                    let split = parent.insert_two_leaves(
                        insert_at,
                        left_idx,
                        left_summary,
                        insert_at,
                        right_idx,
                        right_summary,
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

    pub struct DebugAsBtree<'a, const N: usize, L: Summarize> {
        gtree: &'a Gtree<N, L>,
        inode_idx: InodeIdx,
    }

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
                    "{}{}{}{:?}",
                    &shifts[..shifts.len() - last_shift_byte_len],
                    ident,
                    if gtree.is_inode_deleted(inode_idx) {
                        "🪦 "
                    } else {
                        ""
                    },
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
                            let lnode = gtree.lnode(leaf_idx);
                            writeln!(
                                f,
                                "{}{}{}{:#?}",
                                &shifts,
                                ident,
                                if gtree.is_leaf_deleted(leaf_idx) {
                                    "🪦 "
                                } else {
                                    ""
                                },
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

use inode::{Either, Inode, NodeIdx};

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
        child_summaries: [Leaf::Summary; ARITY],

        /// TODO: docs
        children: [NodeIdx; ARITY],
    }

    /// TODO: docs
    #[derive(Clone, Copy)]
    pub union NodeIdx {
        internal: InodeIdx,
        leaf: LeafIdx,
    }

    impl NodeIdx {
        #[inline]
        const fn dangling() -> Self {
            Self { internal: InodeIdx::dangling() }
        }

        #[inline]
        pub(super) const fn new_internal(internal_idx: InodeIdx) -> Self {
            Self { internal: internal_idx }
        }

        #[inline]
        pub(super) const fn new_leaf(leaf_idx: LeafIdx) -> Self {
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
        pub fn child_at_offset<M>(
            &self,
            at_offset: M,
        ) -> (usize, Leaf::Summary)
        where
            M: Metric<Leaf::Summary>,
        {
            let mut offset = Leaf::Summary::empty();

            for (idx, child_summary) in
                self.child_summaries().iter().copied().enumerate()
            {
                offset += child_summary;

                if M::measure(&offset) >= at_offset {
                    return (idx, offset - child_summary);
                }
            }

            unreachable!();
        }

        /// TODO: docs
        #[inline]
        pub fn child_containing_range<M>(
            &self,
            range: Range<M>,
        ) -> Option<(usize, M)>
        where
            M: Metric<Leaf::Summary>,
        {
            let mut offset = M::zero();

            let summaries = self.child_summaries();

            for (idx, summary) in summaries.iter().enumerate() {
                let child_len = M::measure(summary);

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

        /// TODO: docs
        #[inline]
        pub fn child_measure<M>(&self, child_idx: usize) -> M
        where
            M: Metric<Leaf::Summary>,
        {
            M::measure(self.child_summary(child_idx))
        }

        /// TODO: docs
        #[inline(always)]
        fn child_summaries(&self) -> &[Leaf::Summary] {
            &self.child_summaries[..self.len()]
        }

        /// TODO: docs
        #[inline(always)]
        fn child_summary(&self, child_idx: usize) -> &Leaf::Summary {
            &self.child_summaries[child_idx]
        }

        /// TODO: docs
        #[inline(always)]
        fn child_summary_mut(
            &mut self,
            child_idx: usize,
        ) -> &mut Leaf::Summary {
            &mut self.child_summaries[child_idx]
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

        #[inline]
        pub fn delete_child(&mut self, child_idx: usize) {
            let old_summary = core::mem::replace(
                self.child_summary_mut(child_idx),
                Leaf::Summary::empty(),
            );

            self.summary -= old_summary;
        }

        /// TODO: docs
        #[inline]
        pub fn from_leaf(
            leaf: LeafIdx,
            summary: Leaf::Summary,
            parent: InodeIdx,
        ) -> Self {
            let mut summaries = [Leaf::Summary::empty(); ARITY];
            summaries[0] = summary;

            let mut children = [NodeIdx::dangling(); ARITY];
            children[0] = NodeIdx::new_leaf(leaf);

            Self {
                child_summaries: summaries,
                children,
                parent,
                summary,
                has_leaves: true,
                len: 1,
            }
        }

        /// TODO: docs
        #[inline]
        pub fn from_two_internals(
            first: InodeIdx,
            second: InodeIdx,
            first_summary: Leaf::Summary,
            second_summary: Leaf::Summary,
            parent: InodeIdx,
        ) -> Self {
            let mut children = [NodeIdx::dangling(); ARITY];
            children[0] = NodeIdx::new_internal(first);
            children[1] = NodeIdx::new_internal(second);

            let mut summaries = [Leaf::Summary::empty(); ARITY];
            summaries[0] = first_summary;
            summaries[1] = second_summary;

            Self {
                child_summaries: summaries,
                children,
                parent,
                summary: first_summary + second_summary,
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
            self.insert(
                at_offset,
                NodeIdx::new_internal(inode_idx),
                inode_summary,
            )
        }

        #[inline]
        pub fn insert_leaf(
            &mut self,
            at_offset: usize,
            leaf_idx: LeafIdx,
            leaf_summary: Leaf::Summary,
        ) -> Option<Self> {
            self.insert(at_offset, NodeIdx::new_leaf(leaf_idx), leaf_summary)
        }

        #[inline]
        pub fn insert_two_leaves(
            &mut self,
            first_offset: usize,
            first_leaf: LeafIdx,
            first_summary: Leaf::Summary,
            second_offset: usize,
            second_leaf: LeafIdx,
            second_summary: Leaf::Summary,
        ) -> Option<Self> {
            self.insert_two(
                first_offset,
                NodeIdx::new_leaf(first_leaf),
                first_summary,
                second_offset,
                NodeIdx::new_leaf(second_leaf),
                second_summary,
            )
        }

        #[inline]
        pub fn insert_two(
            &mut self,
            mut first_offset: usize,
            mut first_child: NodeIdx,
            mut first_summary: Leaf::Summary,
            mut second_offset: usize,
            mut second_child: NodeIdx,
            mut second_summary: Leaf::Summary,
        ) -> Option<Self> {
            use core::cmp::Ordering;

            debug_assert!(Self::min_children() >= 2);

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

            debug_assert!(second_offset <= self.len());

            if Self::max_children() - self.len() < 2 {
                let split_offset = self.len() - Self::min_children();

                let children_after_second = self.len() - second_offset;

                // Split so that the extra inode always has the minimum number
                // of children.
                //
                // The logic to make this work is a bit annoying to reason
                // about. We should probably add some unit tests to avoid
                // possible regressions.
                let rest = match children_after_second
                    .cmp(&(Self::min_children() - 1))
                {
                    Ordering::Greater => {
                        let rest = self.split(split_offset);
                        self.insert_two(
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
                        let mut rest = self.split(split_offset + 2);
                        first_offset -= self.len();
                        second_offset -= self.len();
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
                        let mut rest = self.split(split_offset + 1);
                        rest.insert(
                            second_offset - self.len(),
                            second_child,
                            second_summary,
                        );
                        self.insert(first_offset, first_child, first_summary);
                        rest
                    },
                };

                debug_assert_eq!(rest.len(), Self::min_children());

                Some(rest)
            } else {
                self.insert(first_offset, first_child, first_summary);
                self.insert(second_offset + 1, second_child, second_summary);
                None
            }
        }

        /// TODO: docs
        #[inline]
        pub fn insert(
            &mut self,
            at_offset: usize,
            child: NodeIdx,
            child_summary: Leaf::Summary,
        ) -> Option<Self> {
            debug_assert!(at_offset <= self.len());

            if self.is_full() {
                let split_offset = self.len() - Self::min_children();

                // Split so that the extra inode always has the minimum number
                // of children.
                let rest = if at_offset <= Self::min_children() {
                    let rest = self.split(split_offset);
                    self.insert(at_offset, child, child_summary);
                    rest
                } else {
                    let mut rest = self.split(split_offset + 1);
                    rest.insert(at_offset - self.len(), child, child_summary);
                    rest
                };

                debug_assert_eq!(rest.len(), Self::min_children());

                Some(rest)
            } else {
                self.children[at_offset..].rotate_right(1);
                self.children[at_offset] = child;

                self.child_summaries[at_offset..].rotate_right(1);
                self.child_summaries[at_offset] = child_summary;

                self.summary += child_summary;
                self.len += 1;

                None
            }
        }

        #[inline]
        fn is_full(&self) -> bool {
            self.len() == Self::max_children()
        }

        #[inline(always)]
        pub(super) fn len(&self) -> usize {
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
        pub(super) fn parent(&self) -> InodeIdx {
            self.parent
        }

        /// TODO: docs
        #[inline]
        pub(super) fn pushed(
            &mut self,
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
        fn split(&mut self, at_offset: usize) -> Self {
            let len = self.len() - at_offset;

            let children = {
                let mut c = [NodeIdx::dangling(); ARITY];
                c[..len]
                    .copy_from_slice(&self.children[at_offset..self.len()]);
                c
            };

            let child_summaries = {
                let mut cs = [Leaf::Summary::empty(); ARITY];
                cs[..len].copy_from_slice(
                    &self.child_summaries[at_offset..self.len()],
                );
                cs
            };

            let summary = {
                let mut s = Leaf::Summary::empty();
                for &cs in &child_summaries {
                    s += cs;
                }
                s
            };

            self.len = at_offset;

            self.summary -= summary;

            Self {
                child_summaries,
                children,
                has_leaves: self.has_leaves,
                len,
                parent: InodeIdx::dangling(),
                summary,
            }
        }

        #[inline]
        pub fn summary(&self) -> Leaf::Summary {
            self.summary.clone()
        }

        #[inline]
        pub fn update_child_summary(
            &mut self,
            child_idx: usize,
            new_summary: Leaf::Summary,
        ) {
            self.summary += new_summary;

            let old_summary = core::mem::replace(
                &mut self.child_summaries[child_idx],
                new_summary,
            );

            self.summary -= old_summary;
        }

        #[inline]
        pub fn update_summary(
            &mut self,
            patch: <Leaf::Summary as Summary>::Patch,
        ) {
            self.summary.apply_patch(patch);
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

    #[allow(clippy::type_complexity)]
    pub(super) fn insert_at_offset<const N: usize, L, F, M>(
        gtree: &mut Gtree<N, L>,
        in_inode: InodeIdx,
        mut leaf_offset: L::Summary,
        at_offset: M,
        insert_with: F,
    ) -> ((Option<LeafIdx>, Option<LeafIdx>), Option<Inode<N, L>>)
    where
        L: Summarize,
        M: Metric<L::Summary>,
        F: FnOnce(&mut L, M) -> (Option<L>, Option<L>),
    {
        let inode = gtree.inode(in_inode);

        let (child_idx, offset) = inode.child_at_offset(at_offset);

        leaf_offset += offset;

        match inode.child(child_idx) {
            Either::Internal(next_idx) => {
                gtree.with_internal_mut(next_idx, child_idx, |gtree| {
                    insert_at_offset(
                        gtree,
                        next_idx,
                        leaf_offset,
                        at_offset,
                        insert_with,
                    )
                })
            },

            Either::Leaf(leaf_idx) => {
                let (inserted_idx, split_idx, split) =
                    gtree.with_leaf_mut(leaf_idx, child_idx, |leaf| {
                        insert_with(leaf, M::measure(&leaf_offset) - at_offset)
                    });

                gtree.last_insertion_cache = if let Some(idx) = inserted_idx {
                    let summary = gtree.leaf(idx).summarize();
                    Some((idx, leaf_offset + summary))
                } else {
                    Some((leaf_idx, leaf_offset))
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
    pub(super) fn delete_range<
        const N: usize,
        L,
        M,
        DelRange,
        DelFrom,
        DelUpTo,
    >(
        gtree: &mut Gtree<N, L>,
        in_inode: InodeIdx,
        mut range: Range<M>,
        del_range: DelRange,
        del_from: DelFrom,
        del_up_to: DelUpTo,
    ) -> ((Option<LeafIdx>, Option<LeafIdx>), Option<Inode<N, L>>)
    where
        L: Summarize,
        M: Metric<L::Summary>,
        DelRange: FnOnce(&mut L, Range<M>) -> (Option<L>, Option<L>),
        DelFrom: FnOnce(&mut L, M) -> Option<L>,
        DelUpTo: FnOnce(&mut L, M) -> Option<L>,
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

                ((first_idx, second_idx), split)
            },
        }
    }

    #[allow(clippy::type_complexity)]
    fn delete_range_in_inode<const N: usize, L, M, DelFrom, DelUpTo>(
        gtree: &mut Gtree<N, L>,
        idx: InodeIdx,
        range: Range<M>,
        del_from: DelFrom,
        del_up_to: DelUpTo,
    ) -> ((Option<LeafIdx>, Option<LeafIdx>), Option<Inode<N, L>>)
    where
        L: Summarize,
        M: Metric<L::Summary>,
        DelFrom: FnOnce(&mut L, M) -> Option<L>,
        DelUpTo: FnOnce(&mut L, M) -> Option<L>,
    {
        let mut idx_start = 0;
        let mut leaf_idx_start = None;
        let mut extra_from_start = None;

        let mut idx_end = 0;
        let mut leaf_idx_end = None;
        let mut extra_from_end = None;

        let mut idxs = 0..gtree.inode(idx).len();
        let mut offset = M::zero();

        for child_idx in idxs.by_ref() {
            let child_measure = gtree.inode(idx).child_measure(child_idx);

            offset += child_measure;

            if offset > range.start {
                offset -= child_measure;

                idx_start = child_idx;

                match gtree.inode(idx).child(child_idx) {
                    Either::Internal(inode_idx) => {
                        let (leaf_idx, split) = delete_from(
                            gtree,
                            inode_idx,
                            range.start - offset,
                            del_from,
                        );

                        leaf_idx_start = leaf_idx;

                        let new_summary = gtree.inode(inode_idx).summary();
                        let inode = gtree.inode_mut(idx);
                        inode.update_child_summary(child_idx, new_summary);

                        if let Some(mut extra) = split {
                            extra.set_parent(idx);
                            let summary = extra.summary();
                            let inode_idx = gtree.push_inode(extra);
                            let node_idx = NodeIdx::new_internal(inode_idx);
                            extra_from_start = Some((node_idx, summary));
                        }
                    },

                    Either::Leaf(leaf_idx) => {
                        let leaf = gtree.leaf_mut(leaf_idx);
                        let split = del_from(leaf, range.start - offset);

                        let new_summary = leaf.summarize();
                        let inode = gtree.inode_mut(idx);
                        inode.update_child_summary(child_idx, new_summary);

                        if let Some(split) = split {
                            let summary = split.summarize();
                            let leaf_idx = gtree.push_leaf(split, idx);

                            leaf_idx_start = Some(leaf_idx);

                            let node_idx = NodeIdx::new_leaf(leaf_idx);
                            extra_from_start = Some((node_idx, summary));
                        }
                    },
                }

                offset += child_measure;

                break;
            }
        }

        for child_idx in idxs {
            let child_measure = gtree.inode(idx).child_measure(child_idx);

            offset += child_measure;

            if offset >= range.end {
                offset -= child_measure;

                idx_end = child_idx;

                match gtree.inode(idx).child(child_idx) {
                    Either::Internal(inode_idx) => {
                        let (leaf_idx, split) = delete_up_to(
                            gtree,
                            inode_idx,
                            range.end - offset,
                            del_up_to,
                        );

                        leaf_idx_end = leaf_idx;

                        let new_summary = gtree.inode(inode_idx).summary();
                        let inode = gtree.inode_mut(idx);
                        inode.update_child_summary(child_idx, new_summary);

                        if let Some(mut extra) = split {
                            extra.set_parent(idx);
                            let summary = extra.summary();
                            let inode_idx = gtree.push_inode(extra);
                            let node_idx = NodeIdx::new_internal(inode_idx);
                            extra_from_end = Some((node_idx, summary));
                        }
                    },

                    Either::Leaf(leaf_idx) => {
                        let leaf = gtree.leaf_mut(leaf_idx);
                        let split = del_up_to(leaf, range.end - offset);

                        let new_summary = leaf.summarize();
                        let inode = gtree.inode_mut(idx);
                        inode.update_child_summary(child_idx, new_summary);

                        if let Some(split) = split {
                            let summary = split.summarize();
                            let leaf_idx = gtree.push_leaf(split, idx);

                            leaf_idx_end = Some(leaf_idx);

                            let node_idx = NodeIdx::new_leaf(leaf_idx);
                            extra_from_end = Some((node_idx, summary));
                        }
                    },
                }

                break;
            } else {
                gtree.inode_mut(idx).delete_child(child_idx);
            }
        }

        let start_offset = idx_start + 1;

        let end_offset = idx_end + 1;

        let split = match (extra_from_start, extra_from_end) {
            (Some((start, start_summary)), Some((end, end_summary))) => {
                let inode = gtree.inode_mut(idx);
                inode.insert_two(
                    start_offset,
                    start,
                    start_summary,
                    end_offset,
                    end,
                    end_summary,
                )
            },

            (Some((start, summary)), None) => {
                let inode = gtree.inode_mut(idx);
                inode.insert(start_offset, start, summary)
            },

            (None, Some((end, summary))) => {
                let inode = gtree.inode_mut(idx);
                inode.insert(end_offset, end, summary)
            },

            (None, None) => None,
        };

        ((leaf_idx_start, leaf_idx_end), split)
    }

    fn delete_from<const N: usize, L, M, DelFrom>(
        gtree: &mut Gtree<N, L>,
        idx: InodeIdx,
        mut from: M,
        del_from: DelFrom,
    ) -> (Option<LeafIdx>, Option<Inode<N, L>>)
    where
        L: Summarize,
        M: Metric<L::Summary>,
        DelFrom: FnOnce(&mut L, M) -> Option<L>,
    {
        let inode = gtree.inode_mut(idx);

        let mut offset = M::zero();

        for child_idx in 0..inode.len() {
            let child_measure = inode.child_measure(child_idx);

            offset += child_measure;

            if offset > from {
                for idx in child_idx + 1..inode.len() {
                    inode.delete_child(idx);
                }

                offset -= child_measure;

                from -= offset;

                return match inode.child(child_idx) {
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

                        (idx, split)
                    },
                };
            }
        }

        unreachable!();
    }

    fn delete_up_to<const N: usize, L, M, DelUpTo>(
        gtree: &mut Gtree<N, L>,
        idx: InodeIdx,
        mut up_to: M,
        del_up_to: DelUpTo,
    ) -> (Option<LeafIdx>, Option<Inode<N, L>>)
    where
        L: Summarize,
        M: Metric<L::Summary>,
        DelUpTo: FnOnce(&mut L, M) -> Option<L>,
    {
        let inode = gtree.inode_mut(idx);

        let mut offset = M::zero();

        for child_idx in 0..inode.len() {
            let child_measure = inode.child_measure(child_idx);

            offset += child_measure;

            if offset >= up_to {
                offset -= child_measure;

                up_to -= offset;

                return match inode.child(child_idx) {
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

                        (idx, split)
                    },
                };
            } else {
                inode.delete_child(child_idx);
            }
        }

        unreachable!();
    }
}
