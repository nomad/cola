use std::borrow::Cow;

use crate::node::{Inode, Node, Summarize};

/// TODO: docs
pub struct Btree<const ARITY: usize, Leaf: Summarize> {
    root: Node<ARITY, Leaf>,
}

impl<const ARITY: usize, Leaf: Summarize> core::fmt::Debug
    for Btree<ARITY, Leaf>
{
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        core::fmt::Debug::fmt(&self.root, f)
    }
}

impl<const ARITY: usize, Leaf: Summarize> Clone for Btree<ARITY, Leaf>
where
    Leaf: Clone,
    Leaf::Summary: Clone,
{
    #[inline]
    fn clone(&self) -> Self {
        Self { root: self.root.clone() }
    }
}

impl<const ARITY: usize, Leaf: Summarize> From<Leaf> for Btree<ARITY, Leaf> {
    #[inline]
    fn from(leaf: Leaf) -> Self {
        Self { root: Node::Leaf(leaf) }
    }
}

impl<const ARITY: usize, Leaf: Summarize> Btree<ARITY, Leaf> {
    #[inline]
    pub fn replace_root<F>(&mut self, replace_with: F)
    where
        F: FnOnce(Node<ARITY, Leaf>) -> Node<ARITY, Leaf>,
    {
        let dummy_node = Node::Internal(Inode::empty());
        let old_root = core::mem::replace(&mut self.root, dummy_node);
        self.root = replace_with(old_root);
    }

    #[inline]
    pub fn root(&self) -> &Node<ARITY, Leaf> {
        &self.root
    }

    #[inline]
    pub fn root_mut(&mut self) -> &mut Node<ARITY, Leaf> {
        &mut self.root
    }

    #[inline]
    pub fn summary(&self) -> Cow<'_, Leaf::Summary> {
        self.root.summary()
    }
}
