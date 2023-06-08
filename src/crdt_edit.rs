use super::EditRun;

/// An opaque object to be fed to [`Replica::merge()`](crate::Replica::merge).
#[derive(Debug, Clone)]
pub struct CrdtEdit {
    pub(super) inner: CrdtEditInner,
}

impl CrdtEdit {
    #[inline]
    pub(super) fn insertion(fragment: EditRun, content: String) -> Self {
        Self { inner: CrdtEditInner::Insertion { fragment, content } }
    }

    #[inline]
    pub(super) fn noop() -> Self {
        Self { inner: CrdtEditInner::NoOp }
    }
}

#[derive(Debug, Clone)]
pub(super) enum CrdtEditInner {
    Insertion { fragment: EditRun, content: String },

    NoOp,
}