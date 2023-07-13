use crate::{Deletion, Insertion, Replica, ReplicaIdMap, TextEdit};

/// TODO: docs
#[derive(Debug, Clone, Default)]
pub struct BackLog {
    insertions: ReplicaIdMap<InsertionsBackLog>,
    deletions: ReplicaIdMap<DeletionsBackLog>,
}

impl BackLog {
    /// TODO: docs
    #[inline]
    pub fn add_deletion(&mut self, deletion: Deletion) {
        todo!();
    }

    /// TODO: docs
    #[inline]
    pub fn add_insertion(&mut self, insertion: Insertion) {
        todo!();
    }

    /// Creates a new, empty `BackLog`.
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }
}

/// TODO: docs
#[derive(Debug, Clone, Default)]
struct InsertionsBackLog {
    vec: Vec<Insertion>,
}

/// TODO: docs
#[derive(Debug, Clone, Default)]
struct DeletionsBackLog {
    vec: Vec<Deletion>,
}

/// An iterator over the backlogged [`TextEdit`]s that are now ready to be
/// applied to your buffer.
///
/// This struct is created by the [`backlogged`](Replica::backlogged) method on
/// [`Replica`]. See its documentation for more information.
pub struct BackLogged<'a> {
    #[allow(unused)]
    replica: &'a mut Replica,
}

impl<'a> BackLogged<'a> {
    pub(crate) fn from_replica(replica: &'a mut Replica) -> Self {
        Self { replica }
    }
}

impl Iterator for BackLogged<'_> {
    type Item = TextEdit;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        todo!();
    }
}
