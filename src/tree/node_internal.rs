use core::cmp::Ordering;

use super::{Leaf, Metric, Node};

#[derive(Clone)]
pub(super) struct Inode<const N: usize, L: Leaf> {
    children: Vec<Node<N, L>>,
    summary: L::Summary,
}

impl<const N: usize, L: Leaf> core::fmt::Debug for Inode<N, L> {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        pretty_print_inode(self, &mut String::new(), "", 0, f)
    }
}

impl<const ARITY: usize, L: Leaf> Inode<ARITY, L> {
    #[inline]
    pub(super) fn children(&self) -> &[Node<ARITY, L>] {
        &self.children
    }

    #[inline]
    pub(super) fn children_mut(&mut self) -> &mut [Node<ARITY, L>] {
        &mut self.children
    }

    #[inline]
    pub(super) fn empty() -> Self {
        Self { children: Vec::new(), summary: L::Summary::default() }
    }

    #[inline]
    pub(super) fn from_children<C>(children: C) -> Self
    where
        C: Into<Vec<Node<ARITY, L>>>,
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
    pub(super) fn insert(
        &mut self,
        offset: usize,
        child: Node<ARITY, L>,
    ) -> Option<Self> {
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
    pub(super) fn insert_two(
        &mut self,
        offset: usize,
        a: Node<ARITY, L>,
        b: Node<ARITY, L>,
    ) -> Option<Self> {
        if ARITY - self.len() <= 1 {
            let split_offset = self.len() - Self::min_children();

            // Split so that the extra inode always has the minimum number of
            // children.
            let rest =
                match (self.len() - offset).cmp(&(Self::min_children() - 1)) {
                    Ordering::Greater => {
                        let rest = self.split_at(split_offset);
                        self.insert_two(offset, a, b);
                        rest
                    },

                    Ordering::Equal => {
                        let mut rest = self.split_at(split_offset + 1);
                        self.push(a);
                        rest.insert(0, b);
                        rest
                    },

                    Ordering::Less => {
                        let mut rest = self.split_at(split_offset + 2);
                        rest.insert_two(offset - self.len(), a, b);
                        rest
                    },
                };

            debug_assert_eq!(rest.len(), Self::min_children());

            Some(rest)
        } else {
            self.summary += a.summary() + b.summary();
            self.children.splice(offset..offset, [a, b]);
            None
        }
    }

    #[inline]
    pub(super) fn is_full(&self) -> bool {
        self.len() == ARITY
    }

    #[inline]
    pub(super) fn len(&self) -> usize {
        self.children.len()
    }

    #[inline]
    pub(super) fn measure<M: Metric<L>>(&self) -> M {
        M::measure(&self.summary())
    }

    #[inline]
    pub(super) const fn min_children() -> usize {
        ARITY / 2
    }

    #[inline]
    pub(super) fn push(&mut self, child: Node<ARITY, L>) {
        debug_assert!(!self.is_full());
        self.summary += child.summary();
        self.children.push(child);
    }

    #[inline]
    fn split_at(&mut self, offset: usize) -> Self {
        debug_assert!(offset <= self.len());

        let mut new_summary = L::Summary::default();

        let mut other_summary = L::Summary::default();

        for summary in self.children()[..offset].iter().map(Node::summary) {
            new_summary += summary;
        }

        for summary in self.children()[offset..].iter().map(Node::summary) {
            other_summary += summary;
        }

        self.summary = new_summary;

        let other_children = self.children.drain(offset..).collect();

        Self { children: other_children, summary: other_summary }
    }

    #[inline]
    pub(super) fn summary(&self) -> L::Summary {
        self.summary
    }
}

/// Recursively prints a tree-like representation of this node.
#[inline]
fn pretty_print_inode<const N: usize, L: Leaf>(
    inode: &Inode<N, L>,
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
