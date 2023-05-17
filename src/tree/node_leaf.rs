use super::Leaf;

#[derive(Debug, Clone)]
pub(super) struct Lnode<L: Leaf> {
    value: L,
    summary: L::Summary,
}
