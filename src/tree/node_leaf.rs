use super::{Leaf, Metric};

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

impl<L: Leaf> Lnode<L> {
    #[inline]
    pub(super) fn as_pair_mut(&mut self) -> (&mut L, &mut L::Summary) {
        (&mut self.value, &mut self.summary)
    }

    #[inline]
    pub(super) fn measure<M: Metric<L>>(&self) -> M {
        M::measure(self.summary())
    }

    #[inline]
    pub(super) fn summary(&self) -> &L::Summary {
        &self.summary
    }
}
