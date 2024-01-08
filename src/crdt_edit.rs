use crate::*;

/// An insertion in CRDT coordinates.
///
/// This struct is created by the [`inserted`](Replica::inserted) method on the
/// [`Replica`] owned by the peer that performed the insertion, and can be
/// integrated by another [`Replica`] via the
/// [`integrate_insertion`](Replica::integrate_insertion) method.
///
/// See the documentation of those methods for more information.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    any(feature = "encode", feature = "serde"),
    derive(serde::Serialize, serde::Deserialize)
)]
pub struct Insertion {
    /// The anchor point of the insertion.
    anchor: Anchor,

    /// Contains the replica that made the insertion and the temporal range
    /// of the text that was inserted.
    text: Text,

    /// The run timestamp of this insertion.
    run_ts: RunTs,

    /// The Lamport timestamp of this insertion.
    lamport_ts: LamportTs,
}

impl Insertion {
    #[inline(always)]
    pub(crate) fn anchor(&self) -> Anchor {
        self.anchor
    }

    #[inline(always)]
    pub(crate) fn end(&self) -> Length {
        self.text.range.end
    }

    #[inline(always)]
    pub(crate) fn inserted_by(&self) -> ReplicaId {
        self.text.inserted_by()
    }

    #[inline]
    pub(crate) fn is_no_op(&self) -> bool {
        self.len() == 0
    }

    #[inline(always)]
    pub(crate) fn run_ts(&self) -> RunTs {
        self.run_ts
    }

    #[inline(always)]
    pub(crate) fn lamport_ts(&self) -> LamportTs {
        self.lamport_ts
    }

    #[inline]
    pub(crate) fn len(&self) -> Length {
        self.text.len()
    }

    #[inline]
    pub(crate) fn new(
        anchor: Anchor,
        text: Text,
        lamport_ts: LamportTs,
        run_ts: RunTs,
    ) -> Self {
        Self { anchor, text, lamport_ts, run_ts }
    }

    #[inline]
    pub(crate) fn no_op() -> Self {
        Self::new(Anchor::zero(), Text::new(0, 0..0), 0, 0)
    }

    #[inline]
    pub(crate) fn start(&self) -> Length {
        self.text.range.start
    }

    /// The [`Text`] of this insertion.
    #[inline]
    pub fn text(&self) -> &Text {
        &self.text
    }
}

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

    /// The deletion timestamp of this insertion.
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
