use super::{Inode, Leaf, Lnode};

#[derive(Debug, Clone)]
pub(super) enum Node<const N: usize, L: Leaf> {
    Internal(Inode<N, L>),
    Leaf(Lnode<L>),
}
