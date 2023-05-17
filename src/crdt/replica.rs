use core::ops::RangeBounds;

use uuid::Uuid;

use super::{CrdtEdit, Fragment, LamportClock, LocalClock, TextEdit};
use crate::tree::Tree;

const ARITY: usize = 4;

/// TODO: docs
#[derive(Debug, Clone)]
pub struct Replica {
    id: ReplicaId,
    fragment_tree: Tree<ARITY, Fragment>,
    local_clock: LocalClock,
    lamport_clock: LamportClock,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ReplicaId(Uuid);

impl Replica {
    #[inline]
    pub fn contents(&self) -> impl Iterator<Item = &str> {
        core::iter::empty()
    }

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
    pub fn inserted<T>(&mut self, byte_offset: usize, text: T) -> CrdtEdit
    where
        T: Into<String>,
    {
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
    pub fn replaced<R, T>(&mut self, byte_range: R, text: T) -> CrdtEdit
    where
        R: RangeBounds<usize>,
        T: Into<String>,
    {
        todo!();
    }

    /// TODO: docs
    #[inline]
    pub fn undo(&self, crdt_edit: &CrdtEdit) -> CrdtEdit {
        todo!();
    }
}

impl Default for ReplicaId {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl ReplicaId {
    #[inline]
    fn new() -> Self {
        Self(Uuid::new_v4())
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
