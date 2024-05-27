use alloc::collections::VecDeque;
use core::ops::Range;

use crate::*;

/// A [`Replica`]'s backlog of remote edits that have been received from other
/// replicas but have not yet been merged.
///
/// See [`Replica::backlogged`] for more information.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Backlog {
    insertions: ReplicaIdMap<InsertionsBacklog>,
    deletions: ReplicaIdMap<DeletionsBacklog>,
}

impl Backlog {
    pub fn assert_invariants(
        &self,
        version_map: &VersionMap,
        deletion_map: &DeletionMap,
    ) {
        for (&id, insertions) in self.insertions.iter() {
            insertions.assert_invariants(id, version_map);
        }
        for (&id, deletions) in self.deletions.iter() {
            deletions.assert_invariants(id, deletion_map);
        }
    }

    /// Inserts a new [`Deletion`] into the backlog.
    ///
    /// Runs in `O(n)` in the number of deletions already in the backlog, with
    /// a best-case of `O(log n)`.
    ///
    /// # Panics
    ///
    /// Panics if the deletion has already been backlogged.
    #[inline]
    pub fn insert_deletion(&mut self, deletion: Deletion) {
        self.deletions
            .entry(deletion.deleted_by())
            .or_default()
            .insert(deletion);
    }

    /// Inserts a new [`Insertion`] into the backlog.
    ///
    /// Runs in `O(n)` in the number of insertions already in the backlog, with
    /// a best-case of `O(log n)`.
    ///
    /// # Panics
    ///
    /// Panics if the insertion has already been backlogged.
    #[inline]
    pub fn insert_insertion(&mut self, insertion: Insertion) {
        self.insertions
            .entry(insertion.inserted_by())
            .or_default()
            .insert(insertion);
    }

    /// Creates a new, empty `Backlog`.
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }
}

/// Stores the backlogged [`Insertion`]s of a particular replica.
#[derive(Clone, Default, PartialEq)]
struct InsertionsBacklog {
    insertions: VecDeque<Insertion>,
}

impl core::fmt::Debug for InsertionsBacklog {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_list()
            .entries(self.insertions.iter().map(|i| i.text().temporal_range()))
            .finish()
    }
}

impl InsertionsBacklog {
    fn assert_invariants(&self, id: ReplicaId, version_map: &VersionMap) {
        let Some(first) = self.insertions.front() else {
            return;
        };

        assert!(version_map.get(id) <= first.start());

        let mut prev_end = 0;

        for insertion in &self.insertions {
            assert_eq!(insertion.inserted_by(), id);
            assert!(insertion.start() >= prev_end);
            prev_end = insertion.end();
        }
    }

    /// # Panics
    ///
    /// Panics if the insertion has already been inserted.
    #[inline]
    fn insert(&mut self, insertion: Insertion) {
        let offset = self
            .insertions
            .binary_search_by(|probe| probe.start().cmp(&insertion.start()))
            .unwrap_err();

        self.insertions.insert(offset, insertion);
    }
}

/// Stores the backlogged [`Deletion`]s of a particular replica.
#[derive(Clone, Default, PartialEq)]
struct DeletionsBacklog {
    deletions: VecDeque<Deletion>,
}

impl core::fmt::Debug for DeletionsBacklog {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_list()
            .entries(self.deletions.iter().map(|d| d.deletion_ts()))
            .finish()
    }
}

impl DeletionsBacklog {
    fn assert_invariants(&self, id: ReplicaId, deletion_map: &DeletionMap) {
        let Some(first) = self.deletions.front() else {
            return;
        };

        assert!(deletion_map.get(id) <= first.deletion_ts());

        let mut prev_ts = 0;

        for deletion in &self.deletions {
            assert_eq!(deletion.deleted_by(), id);
            assert!(deletion.deletion_ts() > prev_ts);
            prev_ts = deletion.deletion_ts();
        }
    }

    /// # Panics
    ///
    /// Panics if the deletion has already inserted.
    #[inline]
    fn insert(&mut self, deletion: Deletion) {
        let offset = self
            .deletions
            .binary_search_by(|probe| {
                probe.deletion_ts().cmp(&deletion.deletion_ts())
            })
            .unwrap_err();

        self.deletions.insert(offset, deletion);
    }
}

/// An iterator over the backlogged deletions that are ready to be
/// applied to a [`Replica`].
///
/// This struct is created by the
/// [`backlogged_deletions`](Replica::backlogged_deletions) method on
/// [`Replica`]. See its documentation for more information.
pub struct BackloggedDeletions<'a> {
    replica: &'a mut Replica,
    current: Option<&'a mut DeletionsBacklog>,
    iter: ReplicaIdMapValuesMut<'a, DeletionsBacklog>,
}

impl<'a> BackloggedDeletions<'a> {
    #[inline]
    pub(crate) fn from_replica(replica: &'a mut Replica) -> Self {
        let backlog = replica.backlog_mut();

        // We transmute the exclusive reference to the backlog into the same
        // type to get around the borrow checker.
        //
        // SAFETY: this is safe because in the `Iterator` implementation we
        // never access the backlog through the `Replica`, neither directly nor
        // by calling any methods on `Replica` that would access the backlog.
        let backlog = unsafe {
            core::mem::transmute::<&mut Backlog, &mut Backlog>(backlog)
        };

        let mut iter = backlog.deletions.values_mut();

        let current = iter.next();

        Self { replica, iter, current }
    }
}

impl Iterator for BackloggedDeletions<'_> {
    type Item = Vec<Range<Length>>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let deletions = self.current.as_mut()?;

        let Some(first) = deletions.deletions.front() else {
            self.current = self.iter.next();
            return self.next();
        };

        if self.replica.can_merge_deletion(first) {
            let first = deletions.deletions.pop_front().unwrap();
            let ranges = self.replica.merge_unchecked_deletion(&first);
            if ranges.is_empty() {
                self.next()
            } else {
                Some(ranges)
            }
        } else {
            self.current = self.iter.next();
            self.next()
        }
    }
}

impl core::iter::FusedIterator for BackloggedDeletions<'_> {}

/// An iterator over the backlogged insertions that are ready to be
/// applied to a [`Replica`].
///
/// This struct is created by the
/// [`backlogged_insertion`](Replica::backlogged_insertions) method on
/// [`Replica`]. See its documentation for more information.
pub struct BackloggedInsertions<'a> {
    replica: &'a mut Replica,
    current: Option<&'a mut InsertionsBacklog>,
    iter: ReplicaIdMapValuesMut<'a, InsertionsBacklog>,
}

impl<'a> BackloggedInsertions<'a> {
    #[inline]
    pub(crate) fn from_replica(replica: &'a mut Replica) -> Self {
        let backlog = replica.backlog_mut();

        // We transmute the exclusive reference to the backlog into the same
        // type to get around the borrow checker.
        //
        // SAFETY: this is safe because in the `Iterator` implementation we
        // never access the backlog through the `Replica`, neither directly nor
        // by calling any methods on `Replica` that would access the backlog.
        let backlog = unsafe {
            core::mem::transmute::<&mut Backlog, &mut Backlog>(backlog)
        };

        let mut iter = backlog.insertions.values_mut();

        let current = iter.next();

        Self { replica, current, iter }
    }
}

impl Iterator for BackloggedInsertions<'_> {
    type Item = (Text, Length);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let insertions = self.current.as_mut()?;

        let Some(first) = insertions.insertions.front() else {
            self.current = self.iter.next();
            return self.next();
        };

        if self.replica.can_merge_insertion(first) {
            let first = insertions.insertions.pop_front().unwrap();
            let edit = self.replica.merge_unchecked_insertion(&first);
            Some((first.text().clone(), edit))
        } else {
            self.current = self.iter.next();
            self.next()
        }
    }
}

impl core::iter::FusedIterator for BackloggedInsertions<'_> {}

#[cfg(feature = "encode")]
pub(crate) mod encode {
    use super::*;
    use crate::encode::{Decode, DecodeWithCtx, Encode, IntDecodeError};
    use crate::version_map::encode::BaseMapDecodeError;

    impl InsertionsBacklog {
        #[inline(always)]
        fn iter(&self) -> impl Iterator<Item = &Insertion> + '_ {
            self.insertions.iter()
        }

        #[inline(always)]
        fn len(&self) -> usize {
            self.insertions.len()
        }

        #[inline(always)]
        fn push(&mut self, insertion: Insertion) {
            self.insertions.push_back(insertion);
        }
    }

    impl DeletionsBacklog {
        #[inline(always)]
        fn iter(&self) -> impl Iterator<Item = &Deletion> + '_ {
            self.deletions.iter()
        }

        #[inline(always)]
        fn len(&self) -> usize {
            self.deletions.len()
        }

        #[inline(always)]
        fn push(&mut self, deletion: Deletion) {
            self.deletions.push_back(deletion);
        }
    }

    impl Encode for Backlog {
        #[inline]
        fn encode(&self, buf: &mut Vec<u8>) {
            (self.insertions.len() as u64).encode(buf);

            for (id, insertions) in &self.insertions {
                ReplicaIdInsertions::new(*id, insertions).encode(buf);
            }

            (self.deletions.len() as u64).encode(buf);

            for (id, deletions) in &self.deletions {
                ReplicaIdDeletions::new(*id, deletions).encode(buf);
            }
        }
    }

    pub(crate) enum BacklogDecodeError {
        Int(IntDecodeError),
        VersionMap(BaseMapDecodeError<Length>),
    }

    impl From<IntDecodeError> for BacklogDecodeError {
        #[inline(always)]
        fn from(err: IntDecodeError) -> Self {
            Self::Int(err)
        }
    }

    impl From<BaseMapDecodeError<Length>> for BacklogDecodeError {
        #[inline(always)]
        fn from(err: BaseMapDecodeError<Length>) -> Self {
            Self::VersionMap(err)
        }
    }

    impl core::fmt::Display for BacklogDecodeError {
        #[inline]
        fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
            let err: &dyn core::fmt::Display = match self {
                Self::VersionMap(err) => err,
                Self::Int(err) => err,
            };

            write!(f, "Backlog: couldn't be decoded: {err}")
        }
    }

    impl Decode for Backlog {
        type Value = Self;

        type Error = BacklogDecodeError;

        #[inline]
        fn decode(buf: &[u8]) -> Result<(Self::Value, &[u8]), Self::Error> {
            let (num_replicas, mut buf) = u64::decode(buf)?;

            let mut insertions = ReplicaIdMap::default();

            for _ in 0..num_replicas {
                let ((), new_buf) =
                    ReplicaIdInsertions::decode(buf, &mut insertions)?;
                buf = new_buf;
            }

            let (num_replicas, mut buf) = u64::decode(buf)?;

            let mut deletions = ReplicaIdMap::default();

            for _ in 0..num_replicas {
                let ((), new_buf) =
                    ReplicaIdDeletions::decode(buf, &mut deletions)?;
                buf = new_buf;
            }

            let this = Self { insertions, deletions };

            Ok((this, buf))
        }
    }

    struct ReplicaIdInsertions<'a> {
        replica_id: ReplicaId,
        insertions: &'a InsertionsBacklog,
    }

    impl<'a> ReplicaIdInsertions<'a> {
        #[inline]
        fn new(
            replica_id: ReplicaId,
            insertions: &'a InsertionsBacklog,
        ) -> Self {
            Self { replica_id, insertions }
        }
    }

    impl Encode for ReplicaIdInsertions<'_> {
        #[inline]
        fn encode(&self, buf: &mut Vec<u8>) {
            self.replica_id.encode(buf);

            (self.insertions.len() as u64).encode(buf);

            for insertion in self.insertions.iter() {
                insertion.anchor().encode(buf);
                let range = insertion.text().temporal_range();
                range.start.encode(buf);
                range.len().encode(buf);
                insertion.run_ts().encode(buf);
                insertion.lamport_ts().encode(buf);
            }
        }
    }

    impl DecodeWithCtx for ReplicaIdInsertions<'_> {
        type Value = ();

        type Ctx = ReplicaIdMap<InsertionsBacklog>;

        type Error = BacklogDecodeError;

        #[inline]
        fn decode<'buf>(
            buf: &'buf [u8],
            ctx: &mut Self::Ctx,
        ) -> Result<((), &'buf [u8]), Self::Error> {
            let (replica_id, buf) = ReplicaId::decode(buf)?;

            let (num_insertions, mut buf) = u64::decode(buf)?;

            let mut insertions = InsertionsBacklog::default();

            for _ in 0..num_insertions {
                let (anchor, new_buf) = InnerAnchor::decode(buf)?;
                let (start, new_buf) = Length::decode(new_buf)?;
                let (len, new_buf) = Length::decode(new_buf)?;
                let (run_ts, new_buf) = RunTs::decode(new_buf)?;
                let (lamport_ts, new_buf) = LamportTs::decode(new_buf)?;

                let insertion = Insertion::new(
                    anchor,
                    Text::new(replica_id, start..start + len),
                    lamport_ts,
                    run_ts,
                );

                insertions.push(insertion);

                buf = new_buf;
            }

            ctx.insert(replica_id, insertions);

            Ok(((), buf))
        }
    }

    struct ReplicaIdDeletions<'a> {
        replica_id: ReplicaId,
        deletions: &'a DeletionsBacklog,
    }

    impl<'a> ReplicaIdDeletions<'a> {
        #[inline]
        fn new(
            replica_id: ReplicaId,
            deletions: &'a DeletionsBacklog,
        ) -> Self {
            Self { replica_id, deletions }
        }
    }

    impl Encode for ReplicaIdDeletions<'_> {
        #[inline]
        fn encode(&self, buf: &mut Vec<u8>) {
            self.replica_id.encode(buf);

            (self.deletions.len() as u64).encode(buf);

            for deletion in self.deletions.iter() {
                deletion.start().encode(buf);
                deletion.end().encode(buf);
                deletion.version_map().encode(buf);
                deletion.deletion_ts().encode(buf);
            }
        }
    }

    impl DecodeWithCtx for ReplicaIdDeletions<'_> {
        type Value = ();

        type Ctx = ReplicaIdMap<DeletionsBacklog>;

        type Error = BacklogDecodeError;

        #[inline]
        fn decode<'buf>(
            buf: &'buf [u8],
            ctx: &mut Self::Ctx,
        ) -> Result<((), &'buf [u8]), Self::Error> {
            let (replica_id, buf) = ReplicaId::decode(buf)?;

            let (num_deletions, mut buf) = u64::decode(buf)?;

            let mut deletions = DeletionsBacklog::default();

            for _ in 0..num_deletions {
                let (start, new_buf) = InnerAnchor::decode(buf)?;
                let (end, new_buf) = InnerAnchor::decode(new_buf)?;
                let (version_map, new_buf) = VersionMap::decode(new_buf)?;
                let (deletion_ts, new_buf) = DeletionTs::decode(new_buf)?;

                let deletion =
                    Deletion::new(start, end, version_map, deletion_ts);

                deletions.push(deletion);

                buf = new_buf;
            }

            ctx.insert(replica_id, deletions);

            Ok(((), buf))
        }
    }
}

#[cfg(feature = "serde")]
mod serde {
    crate::encode::impl_serialize!(super::Backlog);
    crate::encode::impl_deserialize!(super::Backlog);
}
