use super::{Inode, Leaf, Lnode, Metric, Node};

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

impl<const ARITY: usize, L: Leaf> Tree<ARITY, L> {
    #[inline]
    pub fn insert<M, F>(&mut self, insert_at: M, insert_with: F)
    where
        M: Metric<L>,
        F: FnOnce(M, &mut L) -> (L, Option<L>),
    {
        debug_assert!(insert_at <= self.measure::<M>());

        let root = match &mut self.root {
            Node::Internal(inode) => inode,

            Node::Leaf(lnode) => {
                let (leaf, extra) = insert_with(M::zero(), lnode.value_mut());

                self.replace_root(|old_root| {
                    let leaf = Node::from(leaf);

                    let children = if let Some(extra) = extra {
                        vec![old_root, leaf, Node::from(extra)]
                    } else {
                        vec![old_root, leaf]
                    };

                    Node::from_children(children)
                });

                return;
            },
        };

        if let Some(extra) =
            tree_insert::insert(root, M::zero(), insert_at, insert_with)
        {
            self.replace_root(|old_root| {
                Node::from_children(vec![old_root, Node::Internal(extra)])
            });
        }
    }

    #[inline]
    pub fn measure<M: Metric<L>>(&self) -> M {
        M::measure(&self.summary())
    }

    #[inline]
    fn replace_root<F>(&mut self, replace_with: F)
    where
        F: FnOnce(Node<ARITY, L>) -> Node<ARITY, L>,
    {
        let dummy_node = Node::Internal(Inode::empty());
        let old_root = core::mem::replace(&mut self.root, dummy_node);
        self.root = replace_with(old_root);
    }

    #[inline]
    pub fn summary(&self) -> L::Summary {
        self.root.summary()
    }
}

mod tree_insert {
    use super::*;

    #[inline]
    pub(super) fn insert<const N: usize, L, M, F>(
        inode: &mut Inode<N, L>,
        mut offset: M,
        insert_at: M,
        insert_with: F,
    ) -> Option<Inode<N, L>>
    where
        L: Leaf,
        M: Metric<L>,
        F: FnOnce(M, &mut L) -> (L, Option<L>),
    {
        let mut child_idx = 0;

        let mut extra = None;

        for (idx, child) in inode.children_mut().iter_mut().enumerate() {
            let child_measure = child.measure::<M>();

            if offset + child_measure >= insert_at {
                match child {
                    Node::Internal(child) => {
                        child_idx = idx;
                        extra = insert(child, offset, insert_at, insert_with);
                        break;
                    },

                    Node::Leaf(lnode) => {
                        let (leaf, extra) =
                            insert_with(offset, lnode.value_mut());

                        let leaf = Node::from(leaf);

                        return if let Some(extra) = extra {
                            inode.insert_two(idx + 1, leaf, Node::from(extra))
                        } else {
                            inode.insert(idx + 1, leaf)
                        };
                    },
                }
            } else {
                offset += child_measure;
            }
        }

        if let Some(extra) = extra {
            inode.insert(child_idx + 1, Node::Internal(extra))
        } else {
            None
        }
    }
}
