use crate::*;

/// A text edit in CRDT coordinates.
///
/// This is an opaque type with no public fields or methods. It's created by
/// calling either [`deleted`](crate::Replica::deleted) or
/// [`inserted`](crate::Replica::inserted) on the [`Replica`](crate::Replica)
/// at the peer that originally created the edit, and its only purpose is to be
/// [`merge`](crate::Replica::merge)d by all the other
/// [`Replica`](crate::Replica)s in the same network to create [`TextEdit`]s,
/// which can then be applied to their local text buffers.
///
/// See the [crate-level](crate) documentation or the documentation of any of
/// the methods mentioned above for more information.
#[derive(Debug, Clone)]
pub struct CrdtEdit {
    kind: CrdtEditKind,
}

impl CrdtEdit {
    #[inline]
    pub(super) fn deletion(
        start: Anchor,
        end: Anchor,
        this_id: ReplicaId,
        this_character_ts: Length,
        version_vector: VersionVector,
    ) -> Self {
        let kind = CrdtEditKind::Deletion {
            start,
            end,
            replica_id: this_id,
            character_ts: this_character_ts,
            version_vector,
        };
        Self { kind }
    }

    #[inline]
    pub(super) fn insertion(
        anchor: Anchor,
        this_id: ReplicaId,
        len: Length,
        lamport_ts: LamportTimestamp,
    ) -> Self {
        let kind = CrdtEditKind::Insertion {
            anchor,
            replica_id: this_id,
            len,
            lamport_ts,
        };
        Self { kind }
    }

    #[inline]
    pub(super) fn kind(self) -> CrdtEditKind {
        self.kind
    }

    #[inline]
    pub(super) fn no_op() -> Self {
        Self { kind: CrdtEditKind::NoOp }
    }
}

#[derive(Debug, Clone)]
pub enum CrdtEditKind {
    /// TODO: docs
    Deletion {
        start: Anchor,
        end: Anchor,
        replica_id: ReplicaId,
        character_ts: Length,
        version_vector: VersionVector,
    },

    /// TODO: docs
    Insertion {
        anchor: Anchor,
        replica_id: ReplicaId,
        len: Length,
        lamport_ts: LamportTimestamp,
    },

    NoOp,
}
