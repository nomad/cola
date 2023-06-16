use crate::*;

/// An opaque object to be fed to [`Replica::merge()`](crate::Replica::merge).
#[derive(Debug, Clone)]
pub struct CrdtEdit {
    pub(super) kind: CrdtEditKind,
}

impl CrdtEdit {
    #[inline]
    pub(super) fn continuation(
        content: String,
        len: Length,
        of_insertion: InsertionId,
        old_len: Length,
    ) -> Self {
        Self {
            kind: CrdtEditKind::Continuation {
                content,
                of_insertion,
                old_len,
                len,
            },
        }
    }

    #[inline]
    pub(super) fn new_insertion(
        content: String,
        len: Length,
        id: InsertionId,
        anchor: Anchor,
        lamport_ts: LamportTimestamp,
    ) -> Self {
        Self {
            kind: CrdtEditKind::Insertion {
                content,
                id,
                anchor,
                lamport_ts,
                len,
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
    Continuation {
        content: String,
        len: Length,
        of_insertion: InsertionId,
        old_len: Length,
    },

    Insertion {
        content: String,
        len: Length,
        id: InsertionId,
        anchor: Anchor,
        lamport_ts: LamportTimestamp,
    },

    NoOp,
}
