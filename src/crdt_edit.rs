use crate::*;

/// An opaque object to be fed to [`Replica::merge()`](crate::Replica::merge).
#[derive(Debug, Clone)]
pub struct CrdtEdit {
    pub(super) kind: CrdtEditKind,
}

impl CrdtEdit {
    #[inline]
    pub(super) fn insertion(
        content: String,
        id: InsertionId,
        anchor: Anchor,
        lamport_ts: LamportTimestamp,
    ) -> Self {
        Self {
            kind: CrdtEditKind::Insertion { content, id, anchor, lamport_ts },
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
        content: String,
        id: InsertionId,
        anchor: Anchor,
        lamport_ts: LamportTimestamp,
    },

    NoOp,
}
