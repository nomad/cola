use super::{Leaf, Node};

#[derive(Debug, Clone)]
pub(super) struct Inode<const N: usize, L: Leaf> {
    children: Vec<Node<N, L>>,
    len: usize,
    summary: L::Summary,
}
