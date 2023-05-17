use super::Leaf;

pub(super) struct Lnode<L: Leaf> {
    value: L,
    summary: L::Summary,
}
