use core::fmt::Debug;
use core::ops::{Add, AddAssign, Sub, SubAssign};

pub trait Summarize: Debug {
    type Summary: Debug
        + Default
        + Copy
        + Add<Self::Summary, Output = Self::Summary>
        + Sub<Self::Summary, Output = Self::Summary>
        + AddAssign<Self::Summary>
        + SubAssign<Self::Summary>
        + PartialEq<Self::Summary>;

    fn summarize(&self) -> Self::Summary;
}

pub trait Metric<Leaf: Summarize>:
    Debug
    + Copy
    + Ord
    + Add<Self, Output = Self>
    + Sub<Self, Output = Self>
    + AddAssign<Self>
    + SubAssign<Self>
{
    fn zero() -> Self;

    fn measure_leaf(leaf: &Leaf) -> Self;

    fn measure_summary(summary: &Leaf::Summary) -> Self;
}

#[derive(Clone)]
pub enum Node<const ARITY: usize, Leaf: Summarize> {
    Internal(Inode<ARITY, Leaf>),
    Leaf(Leaf),
}

impl<const ARITY: usize, Leaf: Summarize> Node<ARITY, Leaf> {
    #[inline]
    pub fn as_internal_mut(&mut self) -> &mut Inode<ARITY, Leaf> {
        match self {
            Node::Internal(inode) => inode,
            Node::Leaf(_) => unreachable!(),
        }
    }

    #[inline]
    pub fn as_leaf_mut(&mut self) -> &mut Leaf {
        match self {
            Node::Internal(_) => unreachable!(),
            Node::Leaf(leaf) => leaf,
        }
    }

    #[inline]
    pub fn depth(&self) -> usize {
        match self {
            Node::Internal(inode) => inode.depth(),
            Node::Leaf(_) => 0,
        }
    }

    #[inline]
    pub fn from_children<C>(children: C) -> Self
    where
        C: Into<Vec<Node<ARITY, Leaf>>>,
    {
        Self::Internal(Inode::from_children(children))
    }

    #[inline]
    pub fn is_internal(&self) -> bool {
        matches!(self, Node::Internal(_))
    }

    #[inline]
    pub fn measure<M: Metric<Leaf>>(&self) -> M {
        match self {
            Node::Internal(inode) => inode.measure(),
            Node::Leaf(leaf) => M::measure_leaf(leaf),
        }
    }

    #[inline]
    pub fn summary(&self) -> Leaf::Summary {
        match self {
            Node::Internal(inode) => inode.summary(),
            Node::Leaf(leaf) => leaf.summarize(),
        }
    }
}

#[derive(Clone)]
pub struct Inode<const N: usize, Leaf: Summarize> {
    children: Vec<Node<N, Leaf>>,
    summary: Leaf::Summary,
}

impl<const ARITY: usize, Leaf: Summarize> Inode<ARITY, Leaf> {
    #[inline]
    pub fn children(&self) -> &[Node<ARITY, Leaf>] {
        &self.children
    }

    #[inline]
    pub fn children_mut(&mut self) -> &mut [Node<ARITY, Leaf>] {
        &mut self.children
    }

    #[inline]
    pub fn depth(&self) -> usize {
        let mut depth = 1;

        let mut inode = self;

        while let Some(Node::Internal(first_child)) = inode.children.get(0) {
            inode = first_child;
            depth += 1;
        }

        depth
    }

    #[inline]
    pub fn empty() -> Self {
        Self { children: Vec::new(), summary: Leaf::Summary::default() }
    }

    #[inline]
    pub fn from_children<C>(children: C) -> Self
    where
        C: Into<Vec<Node<ARITY, Leaf>>>,
    {
        let children = children.into();

        if children.is_empty() {
            return Self::empty();
        }

        let mut summary = children[0].summary();

        for child in &children[1..] {
            summary += child.summary();
        }

        Self { children, summary }
    }

    #[inline]
    pub fn insert(
        &mut self,
        offset: usize,
        child: Node<ARITY, Leaf>,
    ) -> Option<Self> {
        debug_assert!(offset <= self.len());

        if self.is_full() {
            let split_offset = self.len() - Self::min_children();

            // Split so that the extra inode always has the minimum number of
            // children.
            let rest = if offset <= Self::min_children() {
                let rest = self.split_at(split_offset);
                self.insert(offset, child);
                rest
            } else {
                let mut rest = self.split_at(split_offset + 1);
                rest.insert(offset - self.len(), child);
                rest
            };

            debug_assert_eq!(rest.len(), Self::min_children());

            Some(rest)
        } else {
            self.summary += child.summary();
            self.children.insert(offset, child);
            None
        }
    }

    #[inline]
    pub fn insert_two(
        &mut self,
        mut offset_a: usize,
        mut a: Node<ARITY, Leaf>,
        mut offset_b: usize,
        mut b: Node<ARITY, Leaf>,
    ) -> Option<Self> {
        use core::cmp::Ordering;

        debug_assert!(Self::min_children() >= 2);

        debug_assert_eq!(self.depth(), a.depth() + 1);

        debug_assert_eq!(a.depth(), b.depth());

        if offset_a > offset_b {
            (a, b, offset_a, offset_b) = (b, a, offset_b, offset_a)
        }

        debug_assert!(offset_b <= self.len());

        if Self::max_children() - self.len() < 2 {
            let split_offset = self.len() - Self::min_children();

            let children_after_b = self.len() - offset_b;

            // Split so that the extra inode always has the minimum number of
            // children.
            //
            // The logic to make this work is a bit annoying to reason about.
            // We should probably add some unit tests to avoid possible
            // regressions.
            let rest = match children_after_b.cmp(&(Self::min_children() - 1))
            {
                Ordering::Greater => {
                    let rest = self.split_at(split_offset);
                    self.insert_two(offset_a, a, offset_b, b);
                    rest
                },

                Ordering::Less if offset_a >= split_offset + 2 => {
                    let mut rest = self.split_at(split_offset + 2);
                    offset_a -= self.len();
                    offset_b -= self.len();
                    rest.insert_two(offset_a, a, offset_b, b);
                    rest
                },

                _ => {
                    let mut rest = self.split_at(split_offset + 1);
                    rest.insert(offset_b - self.len(), b);
                    self.insert(offset_a, a);
                    rest
                },
            };

            debug_assert_eq!(rest.len(), Self::min_children());

            Some(rest)
        } else {
            self.insert(offset_a, a);
            self.insert(offset_b + 1, b);
            None
        }
    }

    #[inline]
    pub fn is_full(&self) -> bool {
        self.len() == ARITY
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.children.len()
    }

    #[inline]
    const fn max_children() -> usize {
        ARITY
    }

    #[inline]
    pub fn measure<M: Metric<Leaf>>(&self) -> M {
        M::measure_summary(&self.summary())
    }

    #[inline]
    const fn min_children() -> usize {
        ARITY / 2
    }

    #[inline]
    pub fn push(&mut self, child: Node<ARITY, Leaf>) {
        debug_assert!(!self.is_full());
        self.summary += child.summary();
        self.children.push(child);
    }

    #[inline]
    fn split_at(&mut self, offset: usize) -> Self {
        debug_assert!(offset <= self.len());

        let summary = if offset <= self.len() {
            let new_summary = sum_summaries(&self.children[..offset]);
            let s = self.summary - new_summary;
            self.summary = new_summary;
            s
        } else {
            let s = sum_summaries(&self.children[offset..]);
            self.summary -= s;
            s
        };

        let children = self.children.drain(offset..).collect();

        Self { children, summary }
    }

    #[inline]
    pub fn summarize(&self) -> Leaf::Summary {
        sum_summaries(self.children())
    }

    #[inline]
    pub fn summary(&self) -> Leaf::Summary {
        self.summary
    }

    #[inline]
    pub fn summary_mut(&mut self) -> &mut Leaf::Summary {
        &mut self.summary
    }

    #[inline]
    pub fn with_child_mut<F, T>(
        &mut self,
        child_idx: usize,
        with_child: F,
    ) -> T
    where
        F: FnOnce(&mut Node<ARITY, Leaf>) -> T,
    {
        let child = &mut self.children[child_idx];
        self.summary -= child.summary();
        let res = with_child(child);
        self.summary += child.summary();
        res
    }
}

#[inline]
fn sum_summaries<const N: usize, Leaf: Summarize>(
    nodes: &[Node<N, Leaf>],
) -> Leaf::Summary {
    let mut summary = Leaf::Summary::default();
    for s in nodes.iter().map(Node::summary) {
        summary += s
    }
    summary
}

impl<const N: usize, Leaf: Summarize> core::fmt::Debug for Node<N, Leaf> {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            Node::Internal(inode) => core::fmt::Debug::fmt(inode, f),
            Node::Leaf(leaf) => core::fmt::Debug::fmt(leaf, f),
        }
    }
}

impl<const N: usize, Leaf: Summarize> core::fmt::Debug for Inode<N, Leaf> {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        pretty_print_inode(self, &mut String::new(), "", 0, f)
    }
}

/// Recursively prints a tree-like representation of this node.
#[inline]
fn pretty_print_inode<const N: usize, Leaf: Summarize>(
    inode: &Inode<N, Leaf>,
    shifts: &mut String,
    ident: &str,
    last_shift_byte_len: usize,
    f: &mut core::fmt::Formatter,
) -> core::fmt::Result {
    writeln!(
        f,
        "{}{}{:?}",
        &shifts[..shifts.len() - last_shift_byte_len],
        ident,
        inode.summary()
    )?;

    for (i, child) in inode.children().iter().enumerate() {
        let is_last = i + 1 == inode.len();
        let ident = if is_last { "└── " } else { "├── " };
        match child {
            Node::Internal(inode) => {
                let shift = if is_last { "    " } else { "│   " };
                shifts.push_str(shift);
                pretty_print_inode(inode, shifts, ident, shift.len(), f)?;
                shifts.truncate(shifts.len() - shift.len());
            },
            Node::Leaf(leaf) => {
                writeln!(f, "{}{}{:#?}", &shifts, ident, &leaf)?;
            },
        }
    }

    Ok(())
}
