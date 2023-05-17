use super::{Leaf, Node};

#[derive(Debug, Clone)]
pub struct Tree<const ARITY: usize, L: Leaf> {
    root: Node<ARITY, L>,
}
