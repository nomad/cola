use core::ops::Range;

use crate::{Length, ReplicaId};

/// TODO: docs
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct Text {
    pub(crate) inserted_by: ReplicaId,
    pub(crate) range: crate::Range<Length>,
}

impl core::fmt::Debug for Text {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:x}.{:?}", self.inserted_by.as_u32(), self.range)
    }
}

impl Text {
    /// TODO: docs
    #[inline]
    pub fn inserted_by(&self) -> ReplicaId {
        self.inserted_by
    }

    #[inline]
    pub(crate) fn new(
        inserted_by: ReplicaId,
        range: crate::Range<Length>,
    ) -> Self {
        Self { inserted_by, range }
    }

    /// TODO: docs
    #[inline]
    pub fn range(&self) -> Range<Length> {
        self.range.into()
    }
}
