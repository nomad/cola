use crate::*;

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
    replica: &'a mut Replica,
    iter: BackLogIter<'a>,
    deletions: Option<ReplicaIdMapValuesMut<'a, DeletionsBackLog>>,
}

/// TODO: docs
enum BackLogIter<'a> {
    Insertions {
        current: Option<&'a mut InsertionsBackLog>,
        iter: ReplicaIdMapValuesMut<'a, InsertionsBackLog>,
    },

    Deletions {
        current: Option<&'a mut DeletionsBackLog>,
        iter: ReplicaIdMapValuesMut<'a, DeletionsBackLog>,
    },
}

impl<'a> BackLogged<'a> {
    #[inline]
    pub(crate) fn from_replica(replica: &'a mut Replica) -> Self {
        let backlog = replica.backlog_mut();

        // We transmute the exclusive reference to the backlog into the same
        // type to get around the borrow checker.
        //
        // SAFETY: this is safe because in the `Iterator` implementation we
        // will never access the backlog through the `Replica` again, neither
        // directly nor by calling any methods on `Replica` that would access
        // the backlog.
        let backlog =
            unsafe { core::mem::transmute::<_, &mut BackLog>(backlog) };

        let mut iter = backlog.insertions.values_mut();

        let deletions = backlog.deletions.values_mut();

        let current = iter.next();

        let iter = BackLogIter::Insertions { current, iter };

        Self { replica, iter, deletions: Some(deletions) }
    }
}

impl Iterator for BackLogged<'_> {
    type Item = TextEdit;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.iter {
            BackLogIter::Insertions { current, iter } => {
                let Some(insertions) = current else {
                    let mut iter = self.deletions.take().unwrap();
                    let current = iter.next();
                    self.iter = BackLogIter::Deletions { current, iter };
                    return self.next();
                };

                let Some(first) = insertions.vec.first() else {
                    *current = iter.next();
                    return self.next();
                };

                if self.replica.can_merge_insertion(first) {
                    let first = insertions.vec.remove(0);
                    let edit = self.replica.merge_unchecked_insertion(&first);
                    Some(edit)
                } else {
                    *current = iter.next();
                    self.next()
                }
            },

            BackLogIter::Deletions { current, iter } => {
                let deletions = current.as_mut()?;

                let Some(first) = deletions.vec.first() else {
                    *current = iter.next();
                    return self.next();
                };

                if self.replica.can_merge_deletion(first) {
                    let first = deletions.vec.remove(0);
                    let edit = self.replica.merge_unchecked_deletion(&first);
                    if edit.is_some() {
                        edit
                    } else {
                        self.next()
                    }
                } else {
                    *current = iter.next();
                    self.next()
                }
            },
        }
    }
}

impl core::iter::FusedIterator for BackLogged<'_> {}
