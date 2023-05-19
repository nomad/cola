use super::{Leaf, Lnode, Metric, Node};

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
    pub fn insert<M, F>(&mut self, at: M, f: F)
    where
        M: Metric<L>,
        F: FnOnce(M, &mut L, &mut L::Summary) -> (L, Option<L>),
    {
        debug_assert!(at <= self.measure::<M>());

        let mut node = &mut self.root;

        let mut offset = M::zero();

        'outer: loop {
            match node {
                Node::Internal(inode) => {
                    for child in inode.children_mut() {
                        let child_measure = child.measure::<M>();

                        if offset + child_measure >= at {
                            node = child;
                            continue 'outer;
                        } else {
                            offset += child_measure;
                        }
                    }

                    unreachable!();
                },

                Node::Leaf(lnode) => {
                    let (leaf, summary) = lnode.as_pair_mut();

                    let (leaf, extra) = f(offset, leaf, summary);

                    println!("inserting {leaf:#?}");

                    return;
                },
            }
        }
    }

    #[inline]
    pub fn measure<M: Metric<L>>(&self) -> M {
        M::measure(self.summary())
    }

    #[inline]
    pub fn summary(&self) -> &L::Summary {
        self.root.summary()
    }
}
