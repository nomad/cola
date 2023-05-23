use super::Fragment;

/// An opaque object to be fed to [`Replica::merge()`](crate::Replica::merge).
#[derive(Debug, Clone)]
pub struct CrdtEdit(pub(super) CrdtEditInner);

impl CrdtEdit {
    #[inline]
    pub(super) fn insertion(fragment: Fragment, content: String) -> Self {
        Self(CrdtEditInner::Insertion { fragment, content })
    }
}

#[derive(Debug, Clone)]
pub(super) enum CrdtEditInner {
    Insertion { fragment: Fragment, content: String },
}
