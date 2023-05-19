use super::{Inode, Leaf, Lnode};

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
