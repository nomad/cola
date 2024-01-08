use crate::*;

/// TODO: docs
#[derive(Copy, Clone, PartialEq, Eq)]
#[cfg_attr(
    any(feature = "encode", feature = "serde"),
    derive(serde::Serialize, serde::Deserialize)
)]
pub struct Anchor {
    /// TODO: docs
    replica_id: ReplicaId,

    /// The [`RunTs`] of the [`EditRun`] containing this [`Anchor`].
    contained_in: RunTs,

    /// TODO: docs
    offset: Length,
}

impl core::fmt::Debug for Anchor {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        if self == &Self::zero() {
            write!(f, "zero")
        } else {
            write!(f, "{:x}.{}", self.replica_id, self.offset)
        }
    }
}

impl Anchor {
    #[inline(always)]
    pub(crate) fn is_zero(&self) -> bool {
        self.replica_id == 0
    }

    #[inline(always)]
    pub(crate) fn new(
        replica_id: ReplicaId,
        offset: Length,
        run_ts: RunTs,
    ) -> Self {
        Self { replica_id, offset, contained_in: run_ts }
    }

    #[inline(always)]
    pub(crate) fn offset(&self) -> Length {
        self.offset
    }

    #[inline(always)]
    pub(crate) fn replica_id(&self) -> ReplicaId {
        self.replica_id
    }

    #[inline(always)]
    pub(crate) fn run_ts(&self) -> RunTs {
        self.contained_in
    }

    /// A special value used to create an anchor at the start of the document.
    #[inline]
    pub const fn zero() -> Self {
        Self { replica_id: 0, offset: 0, contained_in: 0 }
    }
}

/// TODO: docs
#[derive(PartialEq, Eq)]
pub(crate) enum AnchorBias {
    Left,
    Right,
}
