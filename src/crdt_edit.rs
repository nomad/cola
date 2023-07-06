use crate::*;

/// A text edit in CRDT coordinates
///
/// opaque object to be fed to [`Replica::merge()`](crate::Replica::merge).
///
/// TODO: docs
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
        run_len: Length,
        lamport_ts: LamportTimestamp,
    ) -> Self {
        let kind = CrdtEditKind::Insertion {
            anchor,
            replica_id: this_id,
            run_len,
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
        run_len: Length,
        lamport_ts: LamportTimestamp,
    },

    NoOp,
}
