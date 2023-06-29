use crate::gtree::LeafIdx;
use crate::*;

/// TODO: docs
const RUN_TREE_ARITY: usize = 32;

#[derive(Clone, Debug)]
pub struct RunTree {
    pub gtree: Gtree<RUN_TREE_ARITY, EditRun>,
}

impl RunTree {
    #[inline]
    pub fn assert_invariants(&self) {
        self.gtree.assert_invariants();
    }

    #[inline]
    pub fn average_inode_occupancy(&self) -> f32 {
        self.gtree.average_inode_occupancy()
    }

    #[inline]
    pub fn delete(
        &mut self,
        range: Range<u64>,
    ) -> (Option<LeafIdx<EditRun>>, Option<LeafIdx<EditRun>>) {
        let delete_from = EditRun::delete_from;
        let delete_up_to = EditRun::delete_up_to;
        let delete_range = EditRun::delete_range;
        self.gtree.delete(range, delete_range, delete_from, delete_up_to)
    }

    #[inline]
    pub fn count_empty_leaves(&self) -> (usize, usize) {
        self.gtree.count_empty_leaves()
    }

    #[inline]
    pub fn len(&self) -> u64 {
        self.gtree.summary()
    }

    #[inline]
    pub fn new(first_run: EditRun) -> (Self, LeafIdx<EditRun>) {
        let (gtree, idx) = Gtree::new(first_run);
        (Self { gtree }, idx)
    }
}
