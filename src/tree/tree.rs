use super::{Leaf, Lnode, Node};

#[derive(Clone)]
pub struct Tree<const ARITY: usize, L: Leaf> {
    root: Node<ARITY, L>,
}

impl<const ARITY: usize, L: Leaf> core::fmt::Debug for Tree<ARITY, L> {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        core::fmt::Debug::fmt(&self.root, f)
    }
}

impl<const ARITY: usize, L: Leaf> From<L> for Tree<ARITY, L> {
    #[inline]
    fn from(leaf: L) -> Self {
        Self { root: Node::Leaf(Lnode::from(leaf)) }
    }
}

impl<const ARITY: usize, L: Leaf> Tree<ARITY, L> {}
