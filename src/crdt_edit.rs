use crate::*;

/// An opaque object to be fed to [`Replica::merge()`](crate::Replica::merge).
#[derive(Debug, Clone)]
pub struct CrdtEdit {
    pub(super) kind: CrdtEditKind,
}

impl CrdtEdit {
    #[inline]
    pub(super) fn insertion(
        anchor: Anchor,
        this_id: ReplicaId,
        run_len: u64,
        lamport_ts: LamportTimestamp,
    ) -> Self {
        Self {
            kind: CrdtEditKind::Insertion {
                anchor,
                replica_id: this_id,
                run_len,
                lamport_ts,
            },
        }
    }

    #[inline]
    pub(super) fn no_op() -> Self {
        Self { kind: CrdtEditKind::NoOp }
    }
}

#[derive(Debug, Clone)]
pub enum CrdtEditKind {
    Insertion {
        anchor: Anchor,
        replica_id: ReplicaId,
        run_len: u64,
        lamport_ts: LamportTimestamp,
    },

    NoOp,
}
