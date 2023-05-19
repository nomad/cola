use super::Leaf;

#[derive(Clone)]
pub(super) struct Lnode<L: Leaf> {
    value: L,
    summary: L::Summary,
}

impl<L: Leaf> core::fmt::Debug for Lnode<L> {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "{:?}, {:?}", self.value, self.summary)
    }
}

impl<L: Leaf> From<L> for Lnode<L> {
    #[inline]
    fn from(leaf: L) -> Self {
        Self { summary: leaf.summarize(), value: leaf }
    }
}
