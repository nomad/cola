use super::{Inode, Leaf, Lnode, Metric};

#[derive(Clone)]
pub(super) enum Node<const N: usize, L: Leaf> {
    Internal(Inode<N, L>),
    Leaf(Lnode<L>),
}

impl<const ARITY: usize, L: Leaf> core::fmt::Debug for Node<ARITY, L> {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            Node::Internal(inode) => core::fmt::Debug::fmt(inode, f),
            Node::Leaf(leaf) => core::fmt::Debug::fmt(leaf, f),
        }
    }
}

impl<const N: usize, L: Leaf> Node<N, L> {
    #[inline]
    pub(super) fn measure<M: Metric<L>>(&self) -> M {
        match self {
            Node::Internal(inode) => inode.measure(),
            Node::Leaf(leaf) => leaf.measure(),
        }
    }

    #[inline]
    pub(super) fn summary(&self) -> &L::Summary {
        match self {
            Node::Internal(inode) => inode.summary(),
            Node::Leaf(leaf) => leaf.summary(),
        }
    }
}
