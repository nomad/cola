use core::ops::RangeBounds;

use crate::*;

/// TODO: docs
pub type VersionVector = ReplicaIdMap<Length>;

/// A CRDT for text.
///
/// Like all other text CRDTs it allows multiple peers on a distributed
/// network to concurrently edit the same text document, making sure that they
/// all converge to the same final state without relying on a central server to
/// coordinate the edits.
///
/// However, unlike many other CRDTs, a `Replica` doesn't actually store the
/// text contents itself. This allows to decouple the text buffer from the CRDT
/// machinery needed to guarantee convergence in the face of concurrency.
///
/// Put another way, a `Replica` is a pure CRDT that doesn't know anything
/// about where the text is actually stored. This is great because it makes it
/// very easy to use it in conjuction with any text data structure of your
/// choice: simple `String`s, gap buffers, piece tables, ropes, etc.
///
/// When starting a new collaborative editing session, the first peer
/// initializes its `Replica` via the [`new`](Self::new) method and sends it
/// over to the other peers.
///
/// Then, every time a peer performs an edit on their local buffer they inform
/// their `Replica` by calling either [`inserted`](Self::inserted) or
/// [`deleted`](Self::deleted). This produces a [`CrdtEdit`] which can be sent
/// over to the other peers using the network layer of your choice.
///
/// When a peer receives a `CrdtEdit` they can integrate it into their own
/// `Replica` by calling the [`merge`](Self::merge) method. This produces a
/// [`TextEdit`] which informs them *where* in their local buffer they should
/// apply the edit, taking into account all the other edits that have happened
/// concurrently.
///
/// Basically, you tell your `Replica` how your buffer changes, and it tells
/// you how your buffer *should* change when receiving edits from other peers.
pub struct Replica {
    /// TODO: docs
    id: ReplicaId,

    /// TODO: docs
    run_tree: RunTree,

    /// TODO: docs
    run_indices: RunIndices,

    /// TODO: docs
    character_clock: Length,

    /// TODO: docs
    insertion_clock: InsertionClock,

    /// TODO: docs
    lamport_clock: LamportClock,

    /// TODO: docs
    backlog: BackLog,

    /// TODO: docs
    version_vector: VersionVector,
}

impl Replica {
    #[doc(hidden)]
    pub fn assert_invariants(&self) {
        self.run_tree.assert_invariants();
        self.run_indices.assert_invariants(&self.run_tree);
    }

    #[doc(hidden)]
    pub fn average_gtree_inode_occupancy(&self) -> f32 {
        self.run_tree.average_inode_occupancy()
    }

    /// Sometimes the [`merge`](Replica::merge) method is not able to produce a
    /// `TextEdit` for the given `CrdtEdit` at the time it is called. This is
    /// usually because the `CrdtEdit` is itself dependent on some context that
    /// your `Replica` may not have yet.
    ///
    /// When this happens, the `Replica` stores the `CrdtEdit` in an internal
    /// backlog of edits that can't be processed yet, but may be in the future.
    ///
    /// This method returns an iterator over all the backlogged edits which are
    /// now ready to be applied to your buffer.
    ///
    /// The [`BackLogged`] iterator yields [`TextEdit`]s. It's very important
    /// that you apply every `TextEdit` to your buffer in the *exact same*
    /// order in which they were yielded by the iterator. If you don't your
    /// buffer could permanently diverge from the other peers.
    ///
    /// # Example
    /// ```
    /// # use cola::{Replica, TextEdit};
    /// // The buffer at peer 1 is "ab".
    /// let mut replica1 = Replica::new(2);
    ///
    /// // A second peer joins the session.
    /// let mut replica2 = replica1.clone();
    ///
    /// // Peer 1 inserts 'c', 'd', 'e' and 'f' at the end of the buffer.
    /// let insert_c = replica1.inserted(2, 1);
    /// let insert_d = replica1.inserted(3, 1);
    /// let insert_e = replica1.inserted(4, 1);
    /// let insert_f = replica1.inserted(5, 1);
    ///
    /// // For some reason, the network layer messes up the order of the edits
    /// // and they get to the second peer in the opposite order. Because each
    /// // edit depends on the previous one, peer 2 can't merge the insertions
    /// // of the 'd', 'e' and 'f' until it sees the 'c'.
    /// let none_f = replica2.merge(insert_f);
    /// let none_e = replica2.merge(insert_e);
    /// let none_d = replica2.merge(insert_d);
    ///
    /// assert!(none_f.is_none());
    /// assert!(none_e.is_none());
    /// assert!(none_d.is_none());
    ///
    /// // Finally, peer 2 receives the 'c' and it's able merge it right away.
    /// let Some(TextEdit::Insertion(offset_c)) = replica2.merge(insert_c);
    ///
    /// assert_eq!(offset_c, 2);
    ///
    /// // Peer 2 now has all the context it needs to merge the rest of the
    /// // edits that were previously backlogged.
    /// let mut backlogged = replica2.backlogged();
    ///
    /// assert_eq!(backlogged.next(), Some(TextEdit::Insertion(3)));
    /// assert_eq!(backlogged.next(), Some(TextEdit::Insertion(4)));
    /// assert_eq!(backlogged.next(), Some(TextEdit::Insertion(5)));
    /// ```
    #[inline]
    pub fn backlogged(&mut self) -> BackLogged<'_> {
        BackLogged::from_replica(self)
    }

    #[doc(hidden)]
    pub fn debug(&self) -> debug::DebugAsSelf<'_> {
        self.into()
    }

    #[doc(hidden)]
    pub fn debug_as_btree(&self) -> debug::DebugAsBtree<'_> {
        self.into()
    }

    /// Informs your `Replica` that you have deleted the characters in the
    /// given offset range.
    ///
    /// This produces a [`CrdtEdit`] which can be sent to all the other peers
    /// to integrate the deletion into their own `Replica`s.
    ///
    /// # Panics
    ///
    /// Panics if the start of the range is greater than the end or if the end
    /// is out of bounds (i.e. greater than the current length of your buffer).
    ///
    /// # Example
    ///
    /// ```
    /// # use cola::{Replica, TextEdit};
    /// // The buffer at peer 1 is "Hello World".
    /// let mut replica1 = Replica::new(11);
    ///
    /// let mut replica2 = replica1.clone();
    ///
    /// // Peer 1 deletes "Hello ". This produces a `CrdtEdit` which can be
    /// // sent to the peer who owns `replica2` to merge the deletion into
    /// // their buffer.
    /// let edit: CrdtEdit = replica1.deleted(..6);
    /// ```
    #[inline]
    pub fn deleted<R>(&mut self, range: R) -> CrdtEdit
    where
        R: RangeBounds<Length>,
    {
        let (start, end) = range_bounds_to_start_end(range, 0, self.len());

        if start == end {
            return CrdtEdit::no_op();
        }

        let deleted_range = (start..end).into();

        let (start, end, outcome) = self.run_tree.delete(deleted_range);

        match outcome {
            DeletionOutcome::DeletedAcrossRuns { split_start, split_end } => {
                if let Some((replica_id, insertion_ts, offset, idx)) =
                    split_start
                {
                    self.run_indices.get_mut(replica_id).split(
                        insertion_ts,
                        offset,
                        idx,
                    );
                }
                if let Some((replica_id, insertion_ts, offset, idx)) =
                    split_end
                {
                    self.run_indices.get_mut(replica_id).split(
                        insertion_ts,
                        offset,
                        idx,
                    );
                }
            },

            DeletionOutcome::DeletedInMiddleOfSingleRun {
                replica_id,
                insertion_ts,
                range,
                idx_of_deleted,
                idx_of_split,
            } => {
                let indices = self.run_indices.get_mut(replica_id);
                indices.split(insertion_ts, range.start, idx_of_deleted);
                indices.split(insertion_ts, range.end, idx_of_split);
            },

            DeletionOutcome::DeletionSplitSingleRun {
                replica_id,
                insertion_ts,
                offset,
                idx,
            } => self.run_indices.get_mut(replica_id).split(
                insertion_ts,
                offset,
                idx,
            ),

            DeletionOutcome::DeletionMergedInPreviousRun {
                replica_id,
                insertion_ts,
                offset,
                deleted,
            } => {
                self.run_indices.get_mut(replica_id).move_len_to_prev_split(
                    insertion_ts,
                    offset,
                    deleted,
                );
            },

            DeletionOutcome::DeletionMergedInNextRun {
                replica_id,
                insertion_ts,
                offset,
                deleted,
            } => {
                self.run_indices.get_mut(replica_id).move_len_to_next_split(
                    insertion_ts,
                    offset,
                    deleted,
                );
            },

            DeletionOutcome::DeletedWholeRun => {},
        }

        CrdtEdit::deletion(
            start,
            end,
            self.id,
            self.character_clock,
            self.version_vector.clone(),
        )
    }

    #[doc(hidden)]
    pub fn empty_leaves(&self) -> (usize, usize) {
        self.run_tree.count_empty_leaves()
    }

    /// Informs your `Replica` that you have inserted `len` characters at the
    /// given offset.
    ///
    /// This produces a [`CrdtEdit`] which can be sent to all the other peers
    /// to integrate the insertion into their own `Replica`s.
    ///
    /// # Panics
    ///
    /// Panics if the offset is out of bounds (i.e. greater than the current
    /// length of your buffer).
    ///
    /// # Example
    ///
    /// ```
    /// # use cola::{Replica, TextEdit};
    /// // The buffer at peer 1 is "ab".
    /// let mut replica1 = Replica::new(2);
    ///
    /// let mut replica2 = replica1.clone();
    ///
    /// // Peer 1 inserts two characters between the 'a' and the 'b'. This
    /// // produces a `CrdtEdit` which can be sent to the peer who owns
    /// // `replica2` to merge the insertion into their buffer.
    /// let edit: CrdtEdit = replica1.inserted(1, 2);
    /// ```
    #[inline]
    pub fn inserted(&mut self, at_offset: Length, len: Length) -> CrdtEdit {
        if len == 0 {
            return CrdtEdit::no_op();
        }

        let (anchor, outcome) = self.run_tree.insert(
            at_offset,
            len,
            self.character_clock,
            &mut self.insertion_clock,
            &mut self.lamport_clock,
        );

        match outcome {
            InsertionOutcome::ExtendedLastRun => {
                self.run_indices.get_mut(self.id).extend_last(len)
            },

            InsertionOutcome::SplitRun {
                split_id,
                split_insertion,
                split_at_offset,
                split_idx,
                inserted_idx,
            } => {
                self.run_indices.get_mut(self.id).append(len, inserted_idx);

                self.run_indices.get_mut(split_id).split(
                    split_insertion,
                    split_at_offset,
                    split_idx,
                );
            },

            InsertionOutcome::InsertedRun { inserted_idx } => {
                self.run_indices.get_mut(self.id).append(len, inserted_idx)
            },
        };

        let character_ts = self.character_clock;

        self.character_clock += len;

        CrdtEdit::insertion(
            anchor,
            self.id,
            character_ts,
            self.lamport_clock.last(),
            len,
        )
    }

    #[allow(clippy::len_without_is_empty)]
    #[doc(hidden)]
    pub fn len(&self) -> Length {
        self.run_tree.len()
    }

    /// TODO: docs
    #[inline]
    pub fn merge(&mut self, crdt_edit: CrdtEdit) -> Option<TextEdit> {
        match crdt_edit.kind() {
            CrdtEditKind::Insertion {
                anchor,
                replica_id,
                len,
                lamport_ts,
                ..
            } => self.merge_insertion(anchor, replica_id, len, lamport_ts),

            CrdtEditKind::Deletion {
                start,
                end,
                replica_id,
                character_ts,
                version_vector,
            } => self.merge_deletion(
                start,
                end,
                replica_id,
                character_ts,
                version_vector,
            ),

            CrdtEditKind::NoOp => None,
        }
    }

    #[inline]
    fn merge_insertion(
        &mut self,
        _anchor: Anchor,
        _replica: ReplicaId,
        _len: Length,
        lamport_ts: LamportTimestamp,
    ) -> Option<TextEdit> {
        self.lamport_clock.update(lamport_ts);

        todo!();
    }

    #[inline]
    fn merge_deletion(
        &mut self,
        _start: Anchor,
        _end: Anchor,
        _replica: ReplicaId,
        _character_ts: Length,
        _version_vector: VersionVector,
    ) -> Option<TextEdit> {
        todo!();
    }

    /// Creates a new `Replica` from the initial [`Length`] of your buffer.
    ///
    /// Note that if you have multiple peers working on the same document you
    /// should only use this constructor on the first peer, usually the one
    /// that starts the collaboration session.
    ///
    /// The other peers should get their `Replica` from another `Replica`
    /// already in the session by either:
    ///
    /// a) `clone()`ing it if the collaboration happens all in the same process
    /// (e.g. a text editor with plugins running on separate threads),
    ///
    /// b) serializing it and sending it over the network if the collaboration
    /// is between different processes or machines.
    ///
    /// # Example
    /// ```
    /// # use std::thread;
    /// # use cola::Replica;
    /// // A text editor initializes a new Replica on the main thread where the
    /// // buffer is "foo".
    /// let replica_main = Replica::new(3);
    ///
    /// // It then starts a plugin on a separate thread and wants to give it a
    /// // Replica to keep its buffer synchronized with the one on the main
    /// // thread. It does *not* call `new()` again, but instead clones the
    /// // existing Replica and sends it to the new thread.
    /// let replica_plugin = replica_main.clone();
    ///
    /// thread::spawn(move || {
    ///     // The plugin can now use its Replica to exchange edits with the
    ///     // main thread.
    ///     println!("{replica_plugin:?}");
    /// });
    /// ```
    #[inline]
    pub fn new(len: Length) -> Self {
        let replica_id = ReplicaId::new();

        let mut insertion_clock = InsertionClock::new();

        let mut lamport_clock = LamportClock::new();

        let origin_run = EditRun::new(
            Anchor::origin(),
            replica_id,
            (0..len).into(),
            insertion_clock.next(),
            lamport_clock.next(),
        );

        let (run_tree, origin_idx) = RunTree::new(replica_id, origin_run);

        let run_indices = RunIndices::new(replica_id, origin_idx, len);

        Self {
            id: replica_id,
            run_tree,
            run_indices,
            character_clock: len,
            insertion_clock,
            lamport_clock,
            backlog: BackLog::new(),
            version_vector: VersionVector::default(),
        }
    }
}

impl core::fmt::Debug for Replica {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        struct DebugHexU64(u64);

        impl core::fmt::Debug for DebugHexU64 {
            fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
                write!(f, "{:x}", self.0)
            }
        }

        // In the public Debug we just print the ReplicaId to avoid leaking
        // our internals.
        //
        // During development the `Replica::debug()` method (which is public
        // but hidden from the API) can be used to obtain a more useful
        // representation.
        f.debug_tuple("Replica").field(&DebugHexU64(self.id.as_u64())).finish()
    }
}

impl Default for Replica {
    #[inline]
    fn default() -> Self {
        Self::new(0)
    }
}

impl Clone for Replica {
    #[inline(always)]
    fn clone(&self) -> Self {
        let mut lamport_clock = self.lamport_clock;

        lamport_clock.next();

        Self {
            id: ReplicaId::new(),
            run_tree: self.run_tree.clone(),
            character_clock: 0,
            run_indices: self.run_indices.clone(),
            insertion_clock: InsertionClock::new(),
            lamport_clock,
            backlog: self.backlog.clone(),
            version_vector: self.version_vector.clone(),
        }
    }
}

/// TODO: docs
#[derive(Copy, Clone, Default)]
pub struct LamportClock(u64);

impl core::fmt::Debug for LamportClock {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "LamportClock({})", self.0)
    }
}

impl LamportClock {
    #[inline]
    fn last(&self) -> LamportTimestamp {
        self.0.saturating_sub(1)
    }

    #[inline]
    fn new() -> Self {
        Self::default()
    }

    /// TODO: docs
    #[inline]
    pub fn next(&mut self) -> LamportTimestamp {
        let next = self.0;
        self.0 += 1;
        next
    }

    /// TODO: docs
    #[inline]
    fn update(&mut self, other: LamportTimestamp) -> LamportTimestamp {
        self.0 = self.0.max(other) + 1;
        self.0
    }
}

/// TODO: docs
pub type LamportTimestamp = u64;

/// TODO: docs
#[derive(Copy, Clone, Default)]
pub struct InsertionClock(u64);

impl core::fmt::Debug for InsertionClock {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "InsertionClock({})", self.0)
    }
}

impl InsertionClock {
    #[inline]
    fn new() -> Self {
        Self::default()
    }

    /// TODO: docs
    #[inline]
    pub fn next(&mut self) -> InsertionTimestamp {
        let next = self.0;
        self.0 += 1;
        next
    }
}

/// TODO: docs
pub type InsertionTimestamp = u64;

#[inline(always)]
fn range_bounds_to_start_end<R>(
    range: R,
    lo: Length,
    hi: Length,
) -> (Length, Length)
where
    R: RangeBounds<Length>,
{
    use core::ops::Bound;

    let start = match range.start_bound() {
        Bound::Included(&n) => n,
        Bound::Excluded(&n) => n + 1,
        Bound::Unbounded => lo,
    };

    let end = match range.end_bound() {
        Bound::Included(&n) => n + 1,
        Bound::Excluded(&n) => n,
        Bound::Unbounded => hi,
    };

    (start, end)
}

mod debug {
    use core::fmt::Debug;

    use super::*;

    pub struct DebugAsSelf<'a>(BaseDebug<'a, run_tree::DebugAsSelf<'a>>);

    impl<'a> From<&'a Replica> for DebugAsSelf<'a> {
        #[inline]
        fn from(replica: &'a Replica) -> DebugAsSelf<'a> {
            let base = BaseDebug {
                replica,
                debug_run_tree: replica.run_tree.debug_as_self(),
            };

            Self(base)
        }
    }

    impl<'a> core::fmt::Debug for DebugAsSelf<'a> {
        fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
            self.0.fmt(f)
        }
    }

    pub struct DebugAsBtree<'a>(BaseDebug<'a, run_tree::DebugAsBtree<'a>>);

    impl<'a> From<&'a Replica> for DebugAsBtree<'a> {
        #[inline]
        fn from(replica: &'a Replica) -> DebugAsBtree<'a> {
            let base = BaseDebug {
                replica,
                debug_run_tree: replica.run_tree.debug_as_btree(),
            };

            Self(base)
        }
    }

    impl<'a> core::fmt::Debug for DebugAsBtree<'a> {
        fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
            self.0.fmt(f)
        }
    }

    struct BaseDebug<'a, T: Debug> {
        replica: &'a Replica,
        debug_run_tree: T,
    }

    impl<'a, T: Debug> Debug for BaseDebug<'a, T> {
        fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
            let replica = &self.replica;

            f.debug_struct("Replica")
                .field("id", &replica.id)
                .field("run_tree", &self.debug_run_tree)
                .field("run_indices", &replica.run_indices)
                .field("character_clock", &replica.character_clock)
                .field("lamport_clock", &replica.lamport_clock)
                .field("pending", &replica.backlog)
                .finish()
        }
    }
}
