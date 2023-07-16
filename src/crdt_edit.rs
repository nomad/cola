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
        version_map: VersionMap,
        deletion_ts: DeletionTs,
    ) -> Self {
        let kind = CrdtEditKind::Deletion(Deletion {
            start,
            end,
            version_map,
            deletion_ts,
        });
        Self { kind }
    }

    #[inline]
    pub(super) fn insertion(
        anchor: Anchor,
        this_id: ReplicaId,
        start_ts: Length,
        lamport_ts: LamportTimestamp,
        len: Length,
    ) -> Self {
        let kind = CrdtEditKind::Insertion(Insertion {
            anchor,
            replica_id: this_id,
            start_ts,
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
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    any(feature = "encode", feature = "serde"),
    derive(serde::Serialize, serde::Deserialize)
)]
pub struct Insertion {
    pub(crate) anchor: Anchor,
    pub(crate) replica_id: ReplicaId,
    pub(crate) start_ts: Length,
    pub(crate) lamport_ts: LamportTimestamp,
    pub(crate) len: Length,
}

impl Insertion {
    #[inline]
    pub fn inserted_by(&self) -> ReplicaId {
        self.replica_id
    }
}

/// TODO: docs
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    any(feature = "encode", feature = "serde"),
    derive(serde::Serialize, serde::Deserialize)
)]
pub struct Deletion {
    pub(crate) start: Anchor,
    pub(crate) end: Anchor,
    pub(crate) version_map: VersionMap,
    pub(crate) deletion_ts: DeletionTs,
}

impl Deletion {
    #[inline]
    pub fn deleted_by(&self) -> ReplicaId {
        self.version_map.this_id()
    }
}
