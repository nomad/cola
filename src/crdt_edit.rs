use crate::*;

/// A text edit in CRDT coordinates.
///
/// This is an opaque type with no public fields or methods. It's created by
/// calling either [`deleted`](crate::Replica::deleted) or
/// [`inserted`](crate::Replica::inserted) on the [`Replica`](crate::Replica)
/// at the peer that originally created the edit, and its only purpose is to be
/// [`merge`](crate::Replica::merge)d by all the other
/// [`Replica`](crate::Replica)s in the same editing session to create
/// [`TextEdit`]s, which can then be applied to their local text buffers.
///
/// See the the documentation of any of the methods mentioned above for more
/// information.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CrdtEdit {
    #[cfg_attr(feature = "serde", serde(flatten))]
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
        let kind = CrdtEditKind::Deletion(Deletion {
            start,
            end,
            replica_id: this_id,
            character_ts: this_character_ts,
            version_vector,
        });
        Self { kind }
    }

    #[inline]
    pub(super) fn insertion(
        anchor: Anchor,
        this_id: ReplicaId,
        character_ts: Length,
        lamport_ts: LamportTimestamp,
        len: Length,
    ) -> Self {
        let kind = CrdtEditKind::Insertion(Insertion {
            anchor,
            replica_id: this_id,
            character_ts,
            len,
            lamport_ts,
        });
        Self { kind }
    }

    #[inline]
    pub(super) fn kind(&self) -> &CrdtEditKind {
        &self.kind
    }

    #[inline]
    pub(super) fn no_op() -> Self {
        Self { kind: CrdtEditKind::NoOp }
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum CrdtEditKind {
    Deletion(Deletion),
    Insertion(Insertion),
    NoOp,
}

/// TODO: docs
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Insertion {
    pub(crate) anchor: Anchor,
    pub(crate) replica_id: ReplicaId,
    pub(crate) character_ts: Length,
    pub(crate) lamport_ts: LamportTimestamp,
    pub(crate) len: Length,
}

/// TODO: docs
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Deletion {
    pub(crate) start: Anchor,
    pub(crate) end: Anchor,
    pub(crate) replica_id: ReplicaId,
    pub(crate) character_ts: Length,
    pub(crate) version_vector: VersionVector,
}
