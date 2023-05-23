use core::ops::Range;

use crate::node::{Inode, Metric, Node, Summarize};

#[derive(Clone)]
pub struct Tree<const ARITY: usize, Leaf: Summarize> {
    root: Node<ARITY, Leaf>,
}

impl<const ARITY: usize, Leaf: Summarize> core::fmt::Debug
    for Tree<ARITY, Leaf>
{
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        core::fmt::Debug::fmt(&self.root, f)
    }
}

impl<const ARITY: usize, Leaf: Summarize> From<Leaf> for Tree<ARITY, Leaf> {
    #[inline]
    fn from(leaf: Leaf) -> Self {
        Self { root: Node::Leaf(leaf) }
    }
}

impl<const ARITY: usize, Leaf: Summarize> Tree<ARITY, Leaf> {
    #[inline]
    fn measure<M: Metric<Leaf>>(&self) -> M {
        M::measure_summary(&self.summary())
    }

    #[inline]
    fn replace_root<F>(&mut self, replace_with: F)
    where
        F: FnOnce(Node<ARITY, Leaf>) -> Node<ARITY, Leaf>,
    {
        let dummy_node = Node::Internal(Inode::empty());
        let old_root = core::mem::replace(&mut self.root, dummy_node);
        self.root = replace_with(old_root);
    }

    #[inline]
    fn root_mut(&mut self) -> &mut Node<ARITY, Leaf> {
        &mut self.root
    }

    #[inline]
    pub fn summary(&self) -> Leaf::Summary {
        self.root.summary()
    }
}

mod tree_insert {
    use super::*;
    use crate::{Fragment, Replica};

    type Tree = super::Tree<{ Replica::arity() }, Fragment>;
    type Node = super::Node<{ Replica::arity() }, Fragment>;
    type Inode = super::Inode<{ Replica::arity() }, Fragment>;

    impl Tree {
        #[inline]
        pub fn insert<M, F>(&mut self, insert_at: M, insert_with: F)
        where
            M: Metric<Fragment>,
            F: FnOnce(M, &mut Fragment) -> (Fragment, Option<Fragment>),
        {
            debug_assert!(insert_at <= self.measure::<M>());

            let root = match &mut self.root {
                Node::Internal(inode) => inode,

                Node::Leaf(fragment) => {
                    let (leaf, extra) = insert_with(M::zero(), fragment);

                    self.replace_root(|old_root| {
                        let leaf = Node::Leaf(leaf);

                        let children = if let Some(extra) = extra {
                            vec![old_root, leaf, Node::Leaf(extra)]
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
    }

    #[inline]
    pub(super) fn insert<M, F>(
        inode: &mut Inode,
        mut offset: M,
        insert_at: M,
        insert_with: F,
    ) -> Option<Inode>
    where
        M: Metric<Fragment>,
        F: FnOnce(M, &mut Fragment) -> (Fragment, Option<Fragment>),
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

                    Node::Leaf(fragment) => {
                        let (leaf, extra) = insert_with(offset, fragment);

                        let leaf = Node::Leaf(leaf);

                        return if let Some(extra) = extra {
                            inode.insert_two(idx + 1, leaf, Node::Leaf(extra))
                        } else {
                            inode.insert(idx + 1, leaf)
                        };
                    },
                }
            } else {
                offset += child_measure;
            }
        }

        extra.and_then(|e| inode.insert(child_idx + 1, Node::Internal(e)))
    }
}

mod tree_delete {
    use super::*;
    use crate::{Fragment, Replica};

    type Tree = super::Tree<{ Replica::arity() }, Fragment>;
    type Node = super::Node<{ Replica::arity() }, Fragment>;
    type Inode = super::Inode<{ Replica::arity() }, Fragment>;

    impl Tree {
        #[inline]
        pub fn delete(&mut self, delete_range: Range<usize>) {
            let root = match self.root_mut() {
                Node::Internal(inode) => inode,

                Node::Leaf(fragment) => {
                    let (deleted, rest) = fragment.delete_range(delete_range);

                    if let Some(del) = deleted {
                        self.replace_root(|old_root| {
                            let del = Node::Leaf(del);

                            let children = if let Some(rest) = rest {
                                vec![old_root, del, Node::Leaf(rest)]
                            } else {
                                vec![old_root, del]
                            };

                            Node::from_children(children)
                        })
                    }

                    return;
                },
            };

            if let Some(extra) = tree_delete::delete(root, 0, delete_range) {
                self.replace_root(|old_root| {
                    Node::from_children(vec![old_root, Node::Internal(extra)])
                });
            }
        }
    }

    impl Node {
        fn delete(&mut self) {
            match self {
                Node::Internal(inode) => {
                    inode.summary_mut().is_visible = false;
                },

                Node::Leaf(fragment) => fragment.delete(),
            }
        }
    }

    #[inline]
    fn delete(
        inode: &mut Inode,
        mut offset: usize,
        delete_range: Range<usize>,
    ) -> Option<Inode> {
        let mut child_idx = 0;

        let mut extra = None;

        for (idx, child) in inode.children_mut().iter_mut().enumerate() {
            let child_len = child.summary().len;

            offset += child_len;

            if offset >= delete_range.start {
                if offset >= delete_range.end {
                    match child {
                        Node::Internal(child) => {
                            child_idx = idx;

                            extra = delete(
                                child,
                                offset - child_len,
                                delete_range,
                            );

                            break;
                        },

                        Node::Leaf(fragment) => {
                            let (deleted, rest) =
                                fragment.delete_range(delete_range);

                            return if let Some(del) = deleted {
                                let del = Node::Leaf(del);

                                if let Some(rest) = rest {
                                    inode.insert_two(
                                        idx + 1,
                                        del,
                                        Node::Leaf(rest),
                                    )
                                } else {
                                    inode.insert(idx + 1, del)
                                }
                            } else {
                                None
                            };
                        },
                    }
                } else {
                    return delete_range_in_deepest(
                        inode,
                        offset - child_len,
                        delete_range,
                    );
                }
            }
        }

        extra.and_then(|e| inode.insert(child_idx + 1, Node::Internal(e)))
    }

    #[inline]
    fn delete_range_in_deepest(
        inode: &mut Inode,
        mut offset: usize,
        delete_range: Range<usize>,
    ) -> Option<Inode> {
        let mut start_idx = 0;

        let mut extra = None;

        for (idx, child) in inode.children_mut().iter_mut().enumerate() {
            let child_len = child.summary().len;

            offset += child_len;

            if offset >= delete_range.start {
                start_idx = idx;
                extra = something_start(
                    child,
                    offset - child_len,
                    delete_range.start,
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
            let child_len = child.summary().len;

            offset += child_len;

            if offset >= delete_range.end {
                end_idx = start_idx + 1 + idx;
                extra =
                    something_end(child, offset - child_len, delete_range.end);
                break;
            } else {
                child.delete()
            }
        }

        let extra_from_end = extra.and_then(|e| inode.insert(end_idx + 1, e));

        // TODO: document why `extra_from_start` and `extra_from_end` are
        // guaranteed to never be both `Some`s.
        debug_assert!(
            !(extra_from_start.is_some() && extra_from_end.is_some())
        );

        extra_from_start.or(extra_from_end)
    }

    #[inline]
    fn something_start(
        node: &mut Node,
        mut offset: usize,
        delete_from: usize,
    ) -> Option<Node> {
        let inode = match node {
            Node::Internal(inode) => inode,

            Node::Leaf(fragment) => {
                return fragment
                    .delete_from(delete_from - offset)
                    .map(Node::Leaf)
            },
        };

        let mut start_idx = 0;

        let mut extra = None;

        let mut children = inode.children_mut().iter_mut();

        for (idx, child) in children.by_ref().enumerate() {
            let child_len = child.summary().len;

            if offset + child_len >= delete_from {
                start_idx = idx;
                extra = something_start(child, offset, delete_from);
                break;
            } else {
                offset += child_len;
            }
        }

        for child in children {
            child.delete()
        }

        extra.and_then(|e| inode.insert(start_idx + 1, e)).map(Node::Internal)
    }

    #[inline]
    fn something_end(
        node: &mut Node,
        mut offset: usize,
        delete_up_to: usize,
    ) -> Option<Node> {
        let inode = match node {
            Node::Internal(inode) => inode,

            Node::Leaf(fragment) => {
                return fragment
                    .delete_up_to(delete_up_to - offset)
                    .map(Node::Leaf);
            },
        };

        let mut end_idx = 0;

        let mut extra = None;

        for (idx, child) in inode.children_mut().iter_mut().enumerate() {
            let child_len = child.summary().len;

            if offset + child_len >= delete_up_to {
                end_idx = idx;
                extra = something_end(child, offset, delete_up_to);
                break;
            } else {
                child.delete();
                offset += child_len;
            }
        }

        extra.and_then(|e| inode.insert(end_idx + 1, e)).map(Node::Internal)
    }
}
