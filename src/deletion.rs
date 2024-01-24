use crate::anchor::InnerAnchor as Anchor;
use crate::*;

/// A deletion in CRDT coordinates.
///
/// This struct is created by the [`deleted`](Replica::deleted) method on the
/// [`Replica`] owned by the peer that performed the deletion, and can be
/// integrated by another [`Replica`] via the
/// [`integrate_deletion`](Replica::integrate_deletion) method.
///
/// See the documentation of those methods for more information.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    any(feature = "encode", feature = "serde"),
    derive(serde::Serialize, serde::Deserialize)
)]
pub struct Deletion {
    /// The anchor point of the start of the deleted range.
    start: Anchor,

    /// The anchor point of the end of the deleted range.
    end: Anchor,

    /// The version map of the replica at the time of the deletion. This is
    /// used by a `Replica` merging this deletion to determine:
    ///
    /// a) if it has all the text that the `Replica` who created the
    ///   deletion had at the time of the deletion, and
    ///
    /// b) if there's some additional text within the deleted range that
    ///  the `Replica` who created the deletion didn't have at the time
    ///  of the deletion.
    version_map: VersionMap,

    /// The timestamp of this deletion.
    deletion_ts: DeletionTs,
}

impl Deletion {
    #[inline(always)]
    pub(crate) fn deleted_by(&self) -> ReplicaId {
        self.version_map.this_id()
    }

    #[inline(always)]
    pub(crate) fn deletion_ts(&self) -> DeletionTs {
        self.deletion_ts
    }

    #[inline(always)]
    pub(crate) fn end(&self) -> Anchor {
        self.end
    }

    #[inline]
    pub(crate) fn is_no_op(&self) -> bool {
        self.end.is_zero()
    }

    #[inline]
    pub(crate) fn new(
        start: Anchor,
        end: Anchor,
        version_map: VersionMap,
        deletion_ts: DeletionTs,
    ) -> Self {
        Deletion { start, end, version_map, deletion_ts }
    }

    #[inline]
    pub(crate) fn no_op() -> Self {
        Self::new(Anchor::zero(), Anchor::zero(), VersionMap::new(0, 0), 0)
    }

    #[inline(always)]
    pub(crate) fn start(&self) -> Anchor {
        self.start
    }

    #[inline(always)]
    pub(crate) fn version_map(&self) -> &VersionMap {
        &self.version_map
    }
}
