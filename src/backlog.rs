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
        self.deletions.entry(deletion.deleted_by()).or_default().add(deletion);
    }

    /// TODO: docs
    #[inline]
    pub fn add_insertion(&mut self, insertion: Insertion) {
        self.insertions
            .entry(insertion.inserted_by())
            .or_default()
            .add(insertion);
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

impl InsertionsBackLog {
    /// TODO: docs
    #[inline]
    fn add(&mut self, insertion: Insertion) {
        let Err(insert_at_offset) = self
            .vec
            .binary_search_by(|probe| probe.start_ts.cmp(&insertion.start_ts))
        else {
            unreachable!(
                "the start of the insertions produced by a given Replica \
                 form a strictly monotonic sequence so the binary search can \
                 never find an exact match"
            )
        };

        self.vec.insert(insert_at_offset, insertion);
    }
}

/// TODO: docs
#[derive(Debug, Clone, Default)]
struct DeletionsBackLog {
    vec: Vec<Deletion>,
}

impl DeletionsBackLog {
    /// TODO: docs
    #[inline]
    fn add(&mut self, deletion: Deletion) {
        let Err(insert_at_offset) = self.vec.binary_search_by(|probe| {
            probe.deletion_ts.cmp(&deletion.deletion_ts)
        }) else {
            unreachable!(
                "the deletion timestamps produced by a given Replica form a \
                 strictly monotonic sequence so the binary search can never \
                 find an exact match"
            )
        };

        self.vec.insert(insert_at_offset, deletion);
    }
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
