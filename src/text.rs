use core::ops::Range;

use crate::{Length, ReplicaId};

/// TODO: docs
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct Text {
    inserted_by: ReplicaId,
    range: crate::Range<Length>,
}

impl Text {
    /// TODO: docs
    #[inline]
    pub fn inserted_by(&self) -> ReplicaId {
        self.inserted_by
    }

    #[inline]
    pub(crate) fn _new(
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
