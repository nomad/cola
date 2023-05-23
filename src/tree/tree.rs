use core::ops::Range;

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
    pub fn delete<M, F>(&mut self, delete_range: Range<M>, delete_with: F)
    where
        M: Metric<L>,
        F: Fn(M, &mut L) -> Option<L>,
    {
        // Just like when inserting, here we can also have up to N + 2
        // fragments in the tree if we start with N.
        //
        // "abc" -> del 1..2 -> "a", "b", "c"
        //
        // "abc" "def" "ghi"
        //
        // -> del 2..7 ->
        //
        // "a" "bc" "def" "g" "hi"

        if let Some(extra) = tree_delete::delete(
            self.root_mut(),
            M::zero(),
            delete_range,
            delete_with,
        ) {
            self.replace_root(|old_root| {
                Node::from_children(vec![old_root, extra])
            });
        }
    }

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

                *lnode.summary_mut() = lnode.value().summarize();

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
    fn measure<M: Metric<L>>(&self) -> M {
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
    fn root_mut(&mut self) -> &mut Node<ARITY, L> {
        &mut self.root
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

                        *lnode.summary_mut() = lnode.value().summarize();

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

mod tree_delete {
    use super::*;

    #[inline]
    pub(super) fn delete<const N: usize, L, M, F>(
        node: &mut Node<N, L>,
        mut offset: M,
        delete_range: Range<M>,
        delete_with: F,
    ) -> Option<Node<N, L>>
    where
        L: Leaf,
        M: Metric<L>,
        F: Fn(M, &mut L) -> Option<L>,
    {
        let inode = match node {
            Node::Internal(inode) => inode,

            Node::Leaf(lnode) => {
                return delete_with(offset, lnode.value_mut()).map(Node::from)
            },
        };

        let mut child_idx = 0;

        let mut extra = None;

        for (idx, child) in inode.children_mut().iter_mut().enumerate() {
            let child_measure = child.measure::<M>();

            offset += child_measure;

            if offset >= delete_range.start {
                if offset >= delete_range.end {
                    child_idx = idx;
                    extra = delete(
                        child,
                        offset - child_measure,
                        delete_range,
                        delete_with,
                    );
                    break;
                } else {
                    return delete_range_in_deepest(
                        inode,
                        offset - child_measure,
                        delete_range,
                        delete_with,
                    );
                }
            }
        }

        extra.and_then(|e| inode.insert(child_idx + 1, e)).map(Node::Internal)
    }

    #[inline]
    pub(super) fn delete_range_in_deepest<const N: usize, L, M, F>(
        inode: &mut Inode<N, L>,
        mut offset: M,
        delete_range: Range<M>,
        delete_with: F,
    ) -> Option<Node<N, L>>
    where
        L: Leaf,
        M: Metric<L>,
        F: Fn(M, &mut L) -> Option<L>,
    {
        let mut start_idx = 0;

        let mut extra = None;

        for (idx, child) in inode.children_mut().iter_mut().enumerate() {
            let child_measure = child.measure::<M>();

            offset += child_measure;

            if offset >= delete_range.start {
                start_idx = idx;
                extra = something_start(
                    child,
                    offset - child_measure,
                    delete_range.start,
                    &delete_with,
                );
                break;
            }
        }

        let mut extra_from_start =
            extra.and_then(|e| inode.insert(start_idx + 1, e));

        let extra_children = extra_from_start
            .as_mut()
            .map(Inode::children_mut)
            .unwrap_or(&mut []);

        let children = inode.children_mut()[start_idx + 1..]
            .iter_mut()
            .chain(extra_children);

        let mut end_idx = 0;

        let mut extra = None;

        for (idx, child) in children.enumerate() {
            let child_measure = child.measure::<M>();

            offset += child_measure;

            if offset >= delete_range.end {
                end_idx = start_idx + 1 + idx;
                extra = something_end(
                    child,
                    offset - child_measure,
                    delete_range.end,
                    delete_with,
                );
                break;
            } else {
                // TODO: set `is_visible` to false.
            }
        }

        let extra_from_end = extra.and_then(|e| inode.insert(end_idx + 1, e));

        // TODO: document why `extra_from_start` and `extra_from_end` are
        // guaranteed to never be both `Some`s.
        debug_assert!(
            !(extra_from_start.is_some() && extra_from_end.is_some())
        );

        extra_from_start.or(extra_from_end).map(Node::Internal)
    }

    #[inline]
    fn something_start<const N: usize, L, M, F>(
        node: &mut Node<N, L>,
        mut offset: M,
        delete_from: M,
        delete_with: F,
    ) -> Option<Node<N, L>>
    where
        L: Leaf,
        M: Metric<L>,
        F: Fn(M, &mut L) -> Option<L>,
    {
        let inode = match node {
            Node::Internal(inode) => inode,

            Node::Leaf(lnode) => {
                return delete_with(offset, lnode.value_mut()).map(Node::from)
            },
        };

        let mut start_idx = 0;

        let mut extra = None;

        let mut children = inode.children_mut().iter_mut();

        for (idx, child) in children.by_ref().enumerate() {
            let child_measure = child.measure::<M>();

            if offset + child_measure >= delete_from {
                start_idx = idx;
                extra =
                    something_start(child, offset, delete_from, delete_with);
                break;
            } else {
                offset += child_measure;
            }
        }

        for child in children {
            // TODO: set `is_visible` to false.
        }

        extra.and_then(|e| inode.insert(start_idx + 1, e)).map(Node::Internal)
    }

    #[inline]
    fn something_end<const N: usize, L, M, F>(
        node: &mut Node<N, L>,
        mut offset: M,
        delete_up_to: M,
        delete_with: F,
    ) -> Option<Node<N, L>>
    where
        L: Leaf,
        M: Metric<L>,
        F: Fn(M, &mut L) -> Option<L>,
    {
        let inode = match node {
            Node::Internal(inode) => inode,

            Node::Leaf(lnode) => {
                return delete_with(offset, lnode.value_mut()).map(Node::from)
            },
        };

        let mut end_idx = 0;

        let mut extra = None;

        for (idx, child) in inode.children_mut().iter_mut().enumerate() {
            let child_measure = child.measure::<M>();

            if offset + child_measure >= delete_up_to {
                end_idx = idx;
                extra =
                    something_end(child, offset, delete_up_to, delete_with);
                break;
            } else {
                // TODO: set `is_visible` to false.
                offset += child_measure;
            }
        }

        extra.and_then(|e| inode.insert(end_idx + 1, e)).map(Node::Internal)
    }
}
