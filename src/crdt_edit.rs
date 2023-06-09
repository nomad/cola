use crate::*;

/// An opaque object to be fed to [`Replica::merge()`](crate::Replica::merge).
#[derive(Debug, Clone)]
pub struct CrdtEdit {
    pub(super) inner: CrdtEditInner,
}

impl CrdtEdit {
    #[inline]
    pub(super) fn insertion(
        content: String,
        _insertion_id: InsertionAnchor,
        _lamport_ts: LamportTimestamp,
    ) -> Self {
        Self { inner: CrdtEditInner::Insertion { content } }
    }

    #[inline]
    pub(super) fn noop() -> Self {
        Self { inner: CrdtEditInner::NoOp }
    }
}

#[derive(Debug, Clone)]
pub(super) enum CrdtEditInner {
    Insertion { content: String },

    NoOp,
}
