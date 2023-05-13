use core::ops::RangeBounds;

use crate::{CrdtEdit, TextEdit};

/// TODO: docs
#[derive(Clone)]
pub struct Replica {}

impl Replica {
    /// TODO: docs
    #[inline]
    pub fn deleted<R>(&mut self, byte_range: R) -> CrdtEdit
    where
        R: RangeBounds<usize>,
    {
        todo!();
    }

    /// TODO: docs
    #[inline]
    pub fn inserted(&mut self, byte_offset: usize, text: String) -> CrdtEdit {
        todo!();
    }

    /// TODO: docs
    #[inline]
    pub fn new<'a, Chunks>(chunks: Chunks) -> Self
    where
        Chunks: Iterator<Item = &'a str>,
    {
        todo!();
    }

    /// TODO: docs
    #[inline]
    pub fn merge(
        &mut self,
        crdt_edit: &CrdtEdit,
    ) -> impl Iterator<Item = TextEdit> {
        core::iter::empty()
    }

    /// TODO: docs
    #[inline]
    pub fn replaced<R>(&mut self, byte_range: R, text: String) -> CrdtEdit
    where
        R: RangeBounds<usize>,
    {
        todo!();
    }

    /// TODO: docs
    #[inline]
    pub fn undo(&self, crdt_edit: &CrdtEdit) -> CrdtEdit {
        todo!();
    }
}

impl From<&str> for Replica {
    #[inline]
    fn from(s: &str) -> Self {
        Self::new(core::iter::once(s))
    }
}

impl From<String> for Replica {
    #[inline]
    fn from(s: String) -> Self {
        s.as_str().into()
    }
}

impl From<alloc::borrow::Cow<'_, str>> for Replica {
    #[inline]
    fn from(moo: alloc::borrow::Cow<'_, str>) -> Self {
        moo.as_ref().into()
    }
}
