pub use insertion::insert;

use crate::*;

mod insertion {
    use super::*;

    /// TODO: docs
    pub fn insert(
        run_tree: &mut RunTree,
        id_registry: &mut RunIdRegistry,
        insertion_id: InsertionId,
        lamport_ts: LamportTimestamp,
        at_anchor: InsertionAnchor,
        run_id: RunId,
        text_len: usize,
    ) -> usize {
        todo!();
    }
}
