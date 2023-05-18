use super::Leaf;

#[derive(Debug, Clone)]
pub(super) struct Lnode<L: Leaf> {
    value: L,
    summary: L::Summary,
}

impl<L: Leaf> From<L> for Lnode<L> {
    #[inline]
    fn from(leaf: L) -> Self {
        Self { summary: leaf.summarize(), value: leaf }
    }
}
