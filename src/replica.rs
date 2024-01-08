use core::ops::{Range, RangeBounds};

use crate::panic_messages as panic;
use crate::*;

/// A CRDT for text.
///
/// Like all other text CRDTs it allows multiple peers on a distributed
/// network to concurrently edit the same text document, making sure that they
/// all converge to the same final state without relying on a central server to
/// coordinate the edits.
///
/// However, unlike many other CRDTs, a `Replica` doesn't actually store the
/// text contents itself. This allows to decouple the text buffer from the CRDT
/// machinery needed to handle concurrent edits and guarantee convergence.
///
/// Put another way, a `Replica` is a pure CRDT that doesn't know anything
/// about where the text is actually stored. This is great because it makes it
/// very easy to use it together with any text data structure of your choice:
/// simple `String`s, gap buffers, piece tables, ropes, etc.
///
/// # How to distribute `Replica`s between peers.
///
/// When starting a new collaborative editing session, the first peer
/// initializes its `Replica` via the [`new`](Self::new) method,
/// [`encode`](Self::encode)s it and sends the result to the other peers in the
/// session. If a new peer joins the session later on, one of the peers already
/// in the session can [`encode`](Self::encode) their `Replica` and send it to
/// them.
///
/// # How to integrate remote edits.
///
/// Every time a peer performs an edit on their local buffer they must inform
/// their `Replica` by calling either [`inserted`](Self::inserted) or
/// [`deleted`](Self::deleted). This produces [`Insertion`]s and [`Deletion`]s
/// which can be sent over to the other peers using the network layer of your
/// choice.
///
/// When a peer receives a remote `Insertion` or `Deletion` they can integrate
/// it into their own `Replica` by calling either
/// [`integrate_insertion`](Self::integrate_insertion) or
/// [`integrate_deletion`](Self::integrate_deletion), respectively. The output
/// of those methods tells the peer *where* in their local buffer they should
/// apply the edit, taking into account all the other edits that have happened
/// concurrently.
///
/// Basically, you tell your `Replica` how your buffer changes, and it tells
/// you how your buffer *should* change when receiving remote edits.
pub struct Replica {
    /// The unique identifier of this replica.
    id: ReplicaId,

    /// Contains all the [`EditRun`]s that have been applied to this replica so
    /// far. This is the main data structure.
    run_tree: RunTree,

    /// The value of the Lamport clock at this replica.
    lamport_clock: LamportClock,

    /// A local clock that's incremented every time a new insertion run is
    /// created at this replica. If an insertion continues the current run the
    /// clock is not incremented.
    run_clock: RunClock,

    /// Contains the latest character timestamps of all the replicas that this
    /// replica has seen so far.
    version_map: VersionMap,

    /// A clock that keeps track of the order in which insertions happened at
    /// this replica.
    deletion_map: DeletionMap,

    /// A collection of remote edits waiting to be merged.
    backlog: Backlog,
}

impl Replica {
    #[doc(hidden)]
    pub fn assert_invariants(&self) {
        self.run_tree.assert_invariants();
        self.backlog.assert_invariants(&self.version_map, &self.deletion_map);
    }

    #[doc(hidden)]
    pub fn average_gtree_inode_occupancy(&self) -> f32 {
        self.run_tree.average_inode_occupancy()
    }

    /// The [`integrate_deletion`](Replica::integrate_deletion) method is not
    /// able to immediately produce the offset range(s) to be deleted if the
    /// `Deletion` is itself dependent on some context that the `Replica`
    /// doesn't yet have. When this happens the `Deletion` is stored in an
    /// internal backlog of edits that can't be processed yet, but may be in
    /// the future.
    ///
    /// This method returns an iterator over all the backlogged deletions
    /// which are now ready to be applied to your buffer.
    ///
    /// The [`BackloggedDeletions`] iterator yields the same kind of offset
    /// ranges that [`integrate_deletion`](Replica::integrate_deletion) would
    /// have produced had the `Deletion` been integrated right away.
    ///
    /// It's very important for the ranges to be deleted in the exact same
    /// order in which they were yielded by the iterator. If you don't your
    /// buffer could permanently diverge from the other peers.
    ///
    /// # Examples
    ///
    /// ```
    /// # use cola::Replica;
    /// // A buffer with the text "Hello" is replicated between three peers.
    /// let mut replica1 = Replica::new(1, 5);
    /// let mut replica2 = replica1.fork(2);
    /// let mut replica3 = replica2.fork(3);
    ///
    /// // Peer 1 inserts " world!" at the end of the buffer, and after
    /// // integrating the insertion peer 2 deletes "world", leaving only
    /// // "Hello!".
    /// let insert_spc_world_excl = replica1.inserted(5, 7);
    /// let _ = replica2.integrate_insertion(&insert_spc_world_excl);
    /// let delete_world = replica2.deleted(5..11);
    ///
    /// // Peer 3 receives the deletion, but it can't integrate it right away
    /// // because it doesn't have the context it needs. The deletion is stored
    /// // in the backlog.
    /// let ranges = replica3.integrate_deletion(&delete_world);
    ///
    /// assert!(ranges.is_empty());
    ///
    /// // After peer 3 receives the " world!" insertion from peer 1 it can
    /// // finally integrate the deletion.
    /// let _ = replica3.integrate_insertion(&insert_spc_world_excl);
    ///
    /// let mut deletions = replica3.backlogged_deletions();
    /// assert_eq!(deletions.next(), Some(vec![5..11]));
    /// assert_eq!(deletions.next(), None);
    /// ```
    #[inline]
    pub fn backlogged_deletions(&mut self) -> BackloggedDeletions<'_> {
        BackloggedDeletions::from_replica(self)
    }

    /// The [`integrate_insertion`](Replica::integrate_insertion) method is not
    /// able to immediately produce an offset if the `Insertion` is itself
    /// dependent on some context that the `Replica` doesn't yet have. When
    /// this happens the `Insertion` is stored in an internal backlog of edits
    /// that can't be processed yet, but may be in the future.
    ///
    /// This method returns an iterator over all the backlogged insertions
    /// which are now ready to be applied to your buffer.
    ///
    /// The [`BackloggedInsertions`] iterator yields `(Text, Length)` pairs
    /// containing the [`Text`] to be inserted and the offset at which it
    /// should be inserted.
    ///
    /// It's very important for the insertions to be applied in the exact same
    /// order in which they were yielded by the iterator. If you don't your
    /// buffer could permanently diverge from the other peers.
    ///
    /// # Examples
    ///
    /// ```
    /// # use cola::Replica;
    /// // The buffer at peer 1 is "ab".
    /// let mut replica1 = Replica::new(1, 2);
    ///
    /// // A second peer joins the session.
    /// let mut replica2 = replica1.fork(2);
    ///
    /// // Peer 1 inserts 'c', 'd' and 'e' at the end of the buffer.
    /// let insert_c = replica1.inserted(2, 1);
    /// let insert_d = replica1.inserted(3, 1);
    /// let insert_e = replica1.inserted(4, 1);
    ///
    /// // For some reason, the network layer messes up the order of the edits
    /// // and they get to the second peer in the opposite order. Because each
    /// // edit depends on the previous one, peer 2 can't merge the insertions
    /// // of the 'd' and the 'e' until it sees the 'c'.
    /// let none_e = replica2.integrate_insertion(&insert_e);
    /// let none_d = replica2.integrate_insertion(&insert_d);
    ///
    /// assert!(none_e.is_none());
    /// assert!(none_d.is_none());
    ///
    /// // Finally, peer 2 receives the 'c' and it's able merge it right away.
    /// let offset_c = replica2.integrate_insertion(&insert_c).unwrap();
    ///
    /// assert_eq!(offset_c, 2);
    ///
    /// // Peer 2 now has all the context it needs to merge the rest of the
    /// // edits that were previously backlogged.
    /// let mut backlogged = replica2.backlogged_insertions();
    ///
    /// assert!(matches!(backlogged.next(), Some((_, 3))));
    /// assert!(matches!(backlogged.next(), Some((_, 4))));
    /// ```
    #[inline]
    pub fn backlogged_insertions(&mut self) -> BackloggedInsertions<'_> {
        BackloggedInsertions::from_replica(self)
    }

    #[inline]
    pub(crate) fn backlog_mut(&mut self) -> &mut Backlog {
        &mut self.backlog
    }

    /// Returns `true` if this `Replica` is ready to merge the given
    /// `Deletion`.
    #[inline]
    pub(crate) fn can_merge_deletion(&self, deletion: &Deletion) -> bool {
        debug_assert!(!self.has_merged_deletion(deletion));

        (
            // Makes sure that we merge deletions in the same order they were
            // created.
            self.deletion_map.get(deletion.deleted_by()) + 1
                == deletion.deletion_ts()
        ) && (
            // Makes sure that we have already merged all the insertions that
            // the remote `Replica` had when it generated the deletion.
            self.version_map >= *deletion.version_map()
        )
    }

    /// Returns `true` if this `Replica` is ready to merge the given
    /// `Insertion`.
    #[inline]
    pub(crate) fn can_merge_insertion(&self, insertion: &Insertion) -> bool {
        debug_assert!(!self.has_merged_insertion(insertion));

        (
            // Makes sure that we merge insertions in the same order they were
            // created.
            //
            // This is technically not needed to merge a single insertion (all
            // that matters is that we know where to anchor the insertion), but
            // it's needed to correctly increment the chararacter clock inside
            // this `Replica`'s `VersionMap` without skipping any temporal
            // range.
            self.version_map.get(insertion.inserted_by()) == insertion.start()
        ) && (
            // Makes sure that we have already merged the insertion containing
            // the anchor of this insertion.
            self.has_anchor(insertion.anchor())
        )
    }

    /// TODO: doc
    #[track_caller]
    #[inline]
    pub fn create_anchor(&self, at_offset: Length) -> Anchor {
        if at_offset > self.len() {
            panic::offset_out_of_bounds(at_offset, self.len());
        }

        self.run_tree.create_anchor(at_offset)
    }

    #[doc(hidden)]
    pub fn debug(&self) -> debug::DebugAsSelf<'_> {
        self.into()
    }

    #[doc(hidden)]
    pub fn debug_as_btree(&self) -> debug::DebugAsBtree<'_> {
        self.into()
    }

    /// Creates a new `Replica` with the given [`ReplicaId`] by decoding the
    /// contents of the [`EncodedReplica`].
    ///
    /// # Panics
    ///
    /// Panics if the [`ReplicaId`] is zero.
    ///
    /// # Examples
    ///
    /// ```
    /// # use cola::{Replica, EncodedReplica};
    /// let replica1 = Replica::new(1, 42);
    ///
    /// let encoded: EncodedReplica = replica1.encode();
    ///
    /// let replica2 = Replica::decode(2, &encoded).unwrap();
    ///
    /// assert_eq!(replica2.id(), 2);
    /// ```
    #[cfg(feature = "encode")]
    #[cfg_attr(docsrs, doc(cfg(feature = "encode")))]
    #[track_caller]
    #[inline]
    pub fn decode(
        id: ReplicaId,
        encoded: &EncodedReplica,
    ) -> Result<Self, DecodeError> {
        if id == 0 {
            panic::replica_id_is_zero();
        }

        if encoded.protocol_version() != PROTOCOL_VERSION {
            return Err(DecodeError::DifferentProtocol {
                encoded_on: encoded.protocol_version(),
                decoding_on: PROTOCOL_VERSION,
            });
        }

        if encoded.checksum() != checksum_array(encoded.bytes()) {
            return Err(DecodeError::ChecksumFailed);
        }

        let Some((
            run_tree,
            lamport_clock,
            mut version_map,
            mut deletion_map,
            backlog,
        )) = encode::decode(encoded.bytes())
        else {
            return Err(DecodeError::InvalidData);
        };

        version_map.fork_in_place(id, 0);

        deletion_map.fork_in_place(id, 0);

        let replica = Self {
            id,
            run_tree,
            run_clock: RunClock::new(),
            lamport_clock,
            version_map,
            deletion_map,
            backlog,
        };

        Ok(replica)
    }

    /// Informs the `Replica` that you have deleted the characters in the given
    /// offset range.
    ///
    /// This produces a [`Deletion`] which can be sent to all the other peers
    /// to integrate the deletion into their own `Replica`s.
    ///
    /// # Panics
    ///
    /// Panics if the start of the range is greater than the end or if the end
    /// is out of bounds (i.e. greater than the current length of your buffer).
    ///
    /// # Examples
    ///
    /// ```
    /// # use cola::{Replica, Deletion};
    /// // The buffer at peer 1 is "Hello World".
    /// let mut replica1 = Replica::new(1, 11);
    ///
    /// // Peer 1 deletes "Hello ".
    /// let deletion: Deletion = replica1.deleted(..6);
    /// ```
    #[track_caller]
    #[must_use]
    #[inline]
    pub fn deleted<R>(&mut self, range: R) -> Deletion
    where
        R: RangeBounds<Length>,
    {
        let (start, end) = range_bounds_to_start_end(range, 0, self.len());

        if end > self.len() {
            panic::offset_out_of_bounds(end, self.len());
        }

        if start > end {
            panic::start_greater_than_end(start, end);
        }

        if start == end {
            return Deletion::no_op();
        }

        let deleted_range = (start..end).into();

        let (start, start_ts, end, end_ts) =
            self.run_tree.delete(deleted_range);

        *self.deletion_map.this_mut() += 1;

        Deletion::new(
            start,
            start_ts,
            end,
            end_ts,
            self.version_map.clone(),
            self.deletion_map.this(),
        )
    }

    #[doc(hidden)]
    pub fn empty_leaves(&self) -> (usize, usize) {
        self.run_tree.count_empty_leaves()
    }

    /// Returns `true` if the given `Replica` shares the same document state as
    /// this one.
    ///
    /// This is used in tests to make sure that an encode-decode roundtrip was
    /// successful.
    #[doc(hidden)]
    pub fn eq_decoded(&self, other: &Self) -> bool {
        self.run_tree == other.run_tree && self.backlog == other.backlog
    }

    /// Encodes the `Replica` in a custom binary format.
    ///
    /// This can be used to send a `Replica` to another peer over the network.
    /// Once they have received the [`EncodedReplica`] they can decode it via
    /// the [`decode`](Replica::decode) method.
    ///
    /// Note that if you want to collaborate within a single process you can
    /// just [`fork`](Replica::fork) the `Replica` without having to encode it
    /// and decode it again.
    #[cfg(feature = "encode")]
    #[cfg_attr(docsrs, doc(cfg(feature = "encode")))]
    #[inline]
    pub fn encode(&self) -> EncodedReplica {
        let bytes = encode::encode(self);
        let checksum = checksum(&bytes);
        EncodedReplica::new(PROTOCOL_VERSION, checksum, bytes)
    }

    /// Creates a new `Replica` with the given [`ReplicaId`] but with the same
    /// internal state as this one.
    ///
    /// Note that this method should be used when the collaborative session is
    /// limited to a single process (e.g. multiple threads working on the same
    /// document). If you want to collaborate across different processes or
    /// machines you should [`encode`](Replica::encode) the `Replica` and send
    /// the result to the other peers.
    ///
    /// # Panics
    ///
    /// Panics if the [`ReplicaId`] is zero.
    ///
    /// # Examples
    ///
    /// ```
    /// # use cola::{Replica, ReplicaId};
    /// let replica1 = Replica::new(1, 0);
    /// let replica2 = replica1.fork(2);
    /// assert_eq!(replica2.id(), 2)
    /// ```
    #[track_caller]
    #[inline]
    pub fn fork(&self, new_id: ReplicaId) -> Self {
        if new_id == 0 {
            panic::replica_id_is_zero();
        }

        Self {
            id: new_id,
            run_tree: self.run_tree.clone(),
            run_clock: RunClock::new(),
            lamport_clock: self.lamport_clock,
            version_map: self.version_map.fork(new_id, 0),
            deletion_map: self.deletion_map.fork(new_id, 0),
            backlog: self.backlog.clone(),
        }
    }

    /// Returns `true` if this `Replica` contains the given [`Anchor`]
    /// somewhere in its Gtree.
    #[inline]
    fn has_anchor(&self, anchor: Anchor) -> bool {
        self.version_map.get(anchor.replica_id()) >= anchor.character_ts()
    }

    /// Returns `true` if this `Replica` has already merged the given
    /// `Deletion`.
    #[inline]
    fn has_merged_deletion(&self, deletion: &Deletion) -> bool {
        self.deletion_map.get(deletion.deleted_by()) >= deletion.deletion_ts()
    }

    /// Returns `true` if this `Replica` has already merged the given
    /// `Insertion`.
    #[inline]
    fn has_merged_insertion(&self, insertion: &Insertion) -> bool {
        self.version_map.get(insertion.inserted_by()) > insertion.start()
    }

    /// Returns the id of this `Replica`.
    #[inline]
    pub fn id(&self) -> ReplicaId {
        self.id
    }

    /// Informs the `Replica` that you have inserted `len` characters at the
    /// given offset.
    ///
    /// This produces an [`Insertion`] which can be sent to all the other peers
    /// to integrate the insertion into their own `Replica`s.
    ///
    /// # Panics
    ///
    /// Panics if the offset is out of bounds (i.e. greater than the current
    /// length of your buffer).
    ///
    /// # Examples
    ///
    /// ```
    /// # use cola::{Replica, Insertion};
    /// // The buffer at peer 1 is "ab".
    /// let mut replica1 = Replica::new(1, 2);
    ///
    /// // Peer 1 inserts two characters between the 'a' and the 'b'.
    /// let insertion: Insertion = replica1.inserted(1, 2);
    /// ```
    #[track_caller]
    #[must_use]
    #[inline]
    pub fn inserted(&mut self, at_offset: Length, len: Length) -> Insertion {
        if at_offset > self.len() {
            panic::offset_out_of_bounds(at_offset, self.len());
        }

        if len == 0 {
            return Insertion::no_op();
        }

        let start = self.version_map.this();

        *self.version_map.this_mut() += len;

        let end = self.version_map.this();

        let text = Text::new(self.id, start..end);

        let (anchor, anchor_ts) = self.run_tree.insert(
            at_offset,
            text.clone(),
            &mut self.run_clock,
            &mut self.lamport_clock,
        );

        Insertion::new(
            anchor,
            anchor_ts,
            text,
            self.lamport_clock.highest(),
            self.run_clock.last(),
        )
    }

    #[allow(clippy::len_without_is_empty)]
    #[doc(hidden)]
    pub fn len(&self) -> Length {
        self.run_tree.len()
    }

    /// Integrates a remote [`Deletion`] into this `Replica`, returning a
    /// sequence of offset [`Range`]s to be deleted from your buffer.
    ///
    /// The number of ranges can be:
    ///
    /// - zero, if the `Deletion` has already been integrated by this `Replica`
    /// or if it depends on some context that this `Replica` doesn't yet have
    /// (see the [`backlogged_deletions`](Replica::backlogged_deletions) method
    /// which handles this case);
    ///
    /// - one, if there haven't been any concurrent insertions (local or
    /// remote) within the original range of the deletion;
    ///
    /// - more than one, if there have been. In this case the deleted range has
    /// been split into multiple smaller ranges that "skip over" the newly
    /// inserted text.
    ///
    /// The ranges are guaranteed to be sorted in ascending order and to not
    /// overlap, i.e. for any two indices `i` and `j` where `i < j` and `j <
    /// ranges.len()` it holds that `ranges[i].end < ranges[j].start` (and of
    /// course that `ranges[i].start < ranges[i].end`).
    ///
    /// # Examples
    ///
    /// ```
    /// # use cola::Replica;
    /// // Peer 1 starts with the text "abcd" and sends it to a second peer.
    /// let mut replica1 = Replica::new(1, 4);
    ///
    /// let mut replica2 = replica1.fork(2);
    ///
    /// // Peer 1 deletes the "bc" in "abcd".
    /// let deletion = replica1.deleted(1..3);
    ///
    /// // Concurrently, peer 2 inserts a single character at start of the
    /// // document.
    /// let _ = replica2.inserted(0, 1);
    ///
    /// // Now peer 2 receives the deletion from peer 1. Since the previous
    /// // insertion was outside of the deleted region the latter is still
    /// // contiguous at this peer.
    /// let ranges = replica2.integrate_deletion(&deletion);
    ///
    /// assert_eq!(ranges.as_slice(), &[2..4]);
    /// ```
    ///
    /// ```
    /// # use cola::Replica;
    /// // Same as before..
    /// let mut replica1 = Replica::new(1, 4);
    /// let mut replica2 = replica1.fork(2);
    ///
    /// let deletion = replica1.deleted(1..3);
    ///
    /// // ..except now peer 2 inserts a single character between the 'b' and
    /// // the 'c'.
    /// let _ = replica2.inserted(2, 1);
    ///
    /// // Now peer 2 receives the deletion from peer 1. Since the previous
    /// // insertion was inside the deleted range, the latter has now been
    /// // split into two separate ranges.
    /// let ranges = replica2.integrate_deletion(&deletion);
    ///
    /// assert_eq!(ranges.as_slice(), &[1..2, 3..4]);
    /// ```
    #[must_use]
    #[inline]
    pub fn integrate_deletion(
        &mut self,
        deletion: &Deletion,
    ) -> Vec<Range<Length>> {
        if deletion.is_no_op() || self.has_merged_deletion(deletion) {
            Vec::new()
        } else if self.can_merge_deletion(deletion) {
            self.merge_unchecked_deletion(deletion)
        } else {
            self.backlog.insert_deletion(deletion.clone());
            Vec::new()
        }
    }

    /// Integrates a remote [`Insertion`] into this `Replica`, optionally
    /// returning the offset at which to insert the `Insertion`'s
    /// [`Text`](Insertion::text) into your buffer.
    ///
    /// A `None` value can be returned if the `Insertion` has already been
    /// integrated by this `Replica` or if it depends on some context that this
    /// `Replica` doesn't yet have (see the
    /// [`backlogged_insertions`](Replica::backlogged_insertions) method which
    /// handles this case).
    ///
    /// # Examples
    ///
    /// ```
    /// # use cola::{Replica, Insertion};
    /// // Peer 1 starts with the text "ab" and sends it to a second peer.
    /// let mut replica1 = Replica::new(1, 2);
    ///
    /// let mut replica2 = replica1.fork(2);
    ///
    /// // Peer 1 inserts two characters between the 'a' and the 'b'.
    /// let insertion_1 = replica1.inserted(1, 2);
    ///
    /// // Concurrently, peer 2 inserts a character at the start of the
    /// // document.
    /// let insertion_2 = replica2.inserted(0, 1);
    ///
    /// // Peer 1 receives this insertion, and since there haven't been any
    /// // concurrent insertions at the start of the document, its offset
    /// // hasn't changed.
    /// let offset_2 = replica1.integrate_insertion(&insertion_2).unwrap();
    ///
    /// assert_eq!(offset_2, 0);
    ///
    /// // If we try to integrate the same insertion again, we'll get a `None`.
    /// assert!(replica1.integrate_insertion(&insertion_2).is_none());
    ///
    /// // Finally, peer 2 receives the first insertion from peer 1. Its text
    /// // should be inserted between the 'a' and the 'b', which is at offset
    /// // 2 at this peer.
    /// let offset_1 = replica2.integrate_insertion(&insertion_1).unwrap();
    ///
    /// assert_eq!(offset_1, 2);
    /// ```
    #[must_use]
    #[inline]
    pub fn integrate_insertion(
        &mut self,
        insertion: &Insertion,
    ) -> Option<Length> {
        if insertion.is_no_op() || self.has_merged_insertion(insertion) {
            None
        } else if self.can_merge_insertion(insertion) {
            Some(self.merge_unchecked_insertion(insertion))
        } else {
            self.backlog.insert_insertion(insertion.clone());
            None
        }
    }

    /// Merges the given [`Deletion`] without checking whether it can be
    /// merged.
    #[inline]
    pub(crate) fn merge_unchecked_deletion(
        &mut self,
        deletion: &Deletion,
    ) -> Vec<Range<Length>> {
        debug_assert!(self.can_merge_deletion(deletion));

        let ranges = self.run_tree.merge_deletion(deletion);

        *self.deletion_map.get_mut(deletion.deleted_by()) =
            deletion.deletion_ts();

        ranges
    }

    /// Merges the given [`Insertion`] without checking whether it can be
    /// merged.
    #[inline]
    pub(crate) fn merge_unchecked_insertion(
        &mut self,
        insertion: &Insertion,
    ) -> Length {
        debug_assert!(self.can_merge_insertion(insertion));

        let offset = self.run_tree.merge_insertion(insertion);

        *self.version_map.get_mut(insertion.inserted_by()) += insertion.len();

        self.lamport_clock.merge(insertion.lamport_ts());

        offset
    }

    /// Creates a new `Replica` with the given [`ReplicaId`] from the initial
    /// [`Length`] of your buffer.
    ///
    /// Note that if you have multiple peers working on the same document you
    /// should only use this constructor on the first peer, usually the one
    /// that starts the collaboration session.
    ///
    /// The other peers should get their `Replica` from another `Replica`
    /// already in the session by either:
    ///
    /// a) [`fork`](Replica::fork)ing it if the collaboration happens all in
    /// the same process (e.g. a text editor with plugins running on separate
    /// threads),
    ///
    /// b) [`encode`](Replica::encode)ing it and sending the result over the
    /// network if the collaboration is between different processes or
    /// machines.
    ///
    /// # Panics
    ///
    /// Panics if the [`ReplicaId`] is zero.
    ///
    /// # Examples
    ///
    /// ```
    /// # use std::thread;
    /// # use cola::Replica;
    /// // A text editor initializes a new Replica on the main thread where the
    /// // buffer is "foo".
    /// let replica_main = Replica::new(1, 3);
    ///
    /// // It then starts a plugin on a separate thread and wants to give it a
    /// // Replica to keep its buffer synchronized with the one on the main
    /// // thread. It does *not* call `new()` again, but instead forks the
    /// // existing Replica and sends it to the new thread.
    /// let replica_plugin = replica_main.fork(2);
    ///
    /// thread::spawn(move || {
    ///     // The plugin can now use its Replica to exchange edits with the
    ///     // main thread.
    ///     println!("{replica_plugin:?}");
    /// });
    /// ```
    #[track_caller]
    #[inline]
    pub fn new(id: ReplicaId, len: Length) -> Self {
        if id == 0 {
            panic::replica_id_is_zero();
        }

        let mut run_clock = RunClock::new();

        let mut lamport_clock = LamportClock::new();

        let initial_text = Text::new(id, 0..len);

        let first_run =
            EditRun::new(initial_text, run_clock.next(), lamport_clock.next());

        let run_tree = RunTree::new(first_run);

        Self {
            id,
            run_tree,
            run_clock,
            lamport_clock,
            version_map: VersionMap::new(id, len),
            deletion_map: DeletionMap::new(id, 0),
            backlog: Backlog::new(),
        }
    }

    #[doc(hidden)]
    pub fn num_runs(&self) -> usize {
        self.run_tree.count_empty_leaves().1
    }

    /// TODO: doc
    #[inline]
    pub fn resolve_anchor(&self, anchor: Anchor) -> Option<Length> {
        if self.has_anchor(anchor) {
            Some(self.run_tree.resolve_anchor(anchor))
        } else {
            None
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
        f.debug_tuple("Replica").field(&DebugHexU64(self.id)).finish()
    }
}

pub type LamportTs = u64;

/// A distributed logical clock used to determine if a run was in the document
/// when another run was inserted.
///
/// If it was then its [`LamportTs`] is guaranteed to be strictly less than the
/// new run's [`LamportTs`].
///
/// See [this](https://en.wikipedia.org/wiki/Lamport_timestamp) for more.
#[derive(Copy, Clone)]
#[cfg_attr(feature = "encode", derive(serde::Serialize, serde::Deserialize))]
pub struct LamportClock(LamportTs);

impl core::fmt::Debug for LamportClock {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        self.0.fmt(f)
    }
}

impl LamportClock {
    #[inline]
    pub fn highest(&self) -> LamportTs {
        self.0.saturating_sub(1)
    }

    #[inline]
    fn merge(&mut self, remote_ts: LamportTs) {
        if remote_ts >= self.0 {
            self.0 = remote_ts + 1;
        }
    }

    #[inline]
    fn new() -> Self {
        Self(0)
    }

    #[inline]
    pub fn next(&mut self) -> LamportTs {
        let next = self.0;
        self.0 += 1;
        next
    }
}

pub type RunTs = u64;

/// A local clock that's increased every time a new insertion run is started.
#[derive(Copy, Clone)]
pub struct RunClock(RunTs);

impl core::fmt::Debug for RunClock {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        self.0.fmt(f)
    }
}

impl RunClock {
    #[inline]
    fn last(&self) -> RunTs {
        self.0.saturating_sub(1)
    }

    #[inline]
    fn new() -> Self {
        Self(0)
    }

    #[inline]
    pub fn next(&mut self) -> RunTs {
        let next = self.0;
        self.0 += 1;
        next
    }
}

pub type DeletionTs = u64;

#[cfg(feature = "encode")]
mod encode {
    use serde::{de, ser};

    use super::*;

    type EncodedFields =
        (RunTree, LamportClock, VersionMap, DeletionMap, Backlog);

    #[inline]
    pub(super) fn encode(replica: &Replica) -> Box<[u8]> {
        let mut encoded = Vec::new();

        encode_field(&mut encoded, &replica.run_tree);
        encode_field(&mut encoded, &replica.lamport_clock);
        encode_field(&mut encoded, &replica.version_map);
        encode_field(&mut encoded, &replica.deletion_map);
        encode_field(&mut encoded, &replica.backlog);

        encoded.into_boxed_slice()
    }

    #[inline]
    pub(super) fn decode(bytes: &[u8]) -> Option<EncodedFields> {
        let (run_tree, bytes) = decode_field(bytes)?;
        let (lamport_clock, bytes) = decode_field(bytes)?;
        let (version_map, bytes) = decode_field(bytes)?;
        let (deletion_map, bytes) = decode_field(bytes)?;
        let (backlog, bytes) = decode_field(bytes)?;

        if bytes.is_empty() {
            Some((run_tree, lamport_clock, version_map, deletion_map, backlog))
        } else {
            None
        }
    }

    #[inline]
    fn encode_field<T>(buf: &mut Vec<u8>, field: &T)
    where
        T: ser::Serialize,
    {
        // Make room for the length of the encoded field.
        let len_bytes = [0; 8];
        buf.extend_from_slice(&len_bytes);

        let len_before = buf.len();
        serialize(buf, field);
        let len_after = buf.len();

        // We convert the usize into a u64 before turning it into a byte array
        // to guarantee that the latter is always 8 bytes long. If we don't the
        // length will vary depending on the architecture.
        let len_bytes = ((len_after - len_before) as u64).to_le_bytes();

        buf[len_before - 8..len_before].copy_from_slice(&len_bytes);
    }

    #[inline]
    fn decode_field<'a, T>(buf: &'a [u8]) -> Option<(T, &'a [u8])>
    where
        T: de::Deserialize<'a>,
    {
        // The first 8 bytes represent the length of the encoded field.
        let (len_bytes, rest) = if buf.len() >= 8 {
            buf.split_at(8)
        } else {
            return None;
        };

        let len_bytes: [u8; 8] = len_bytes.try_into().ok()?;

        let len = u64::from_le_bytes(len_bytes) as usize;

        let (encoded_field, rest) = if rest.len() >= len {
            rest.split_at(len)
        } else {
            return None;
        };

        deserialize::<T>(encoded_field).map(|field| (field, rest))
    }

    #[inline]
    fn serialize<T>(buf: &mut Vec<u8>, value: &T)
    where
        T: ser::Serialize,
    {
        bincode::serialize_into(buf, value).expect("failed to serialize")
    }

    #[inline]
    fn deserialize<'a, T>(bytes: &'a [u8]) -> Option<T>
    where
        T: de::Deserialize<'a>,
    {
        bincode::deserialize(bytes).ok()
    }
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
                .field("run_indices", &replica.run_tree.run_indices())
                .field("lamport_clock", &replica.lamport_clock)
                .field("run_clock", &replica.run_clock)
                .field("version_map", &replica.version_map)
                .field("deletion_map", &replica.deletion_map)
                .field("backlog", &replica.backlog)
                .finish()
        }
    }
}
