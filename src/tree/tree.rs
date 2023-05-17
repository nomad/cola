use super::{Leaf, Node};

pub struct Tree<const ARITY: usize, L: Leaf> {
    root: Node<ARITY, L>,
}
