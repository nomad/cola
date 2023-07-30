use crate::*;

/// A text edit in CRDT coordinates.
///
/// This is an opaque type with no public fields or methods. It's created by
/// calling either [`deleted`](crate::Replica::deleted) or
/// [`inserted`](crate::Replica::inserted) on the [`Replica`](crate::Replica)
/// at the peer that originally created the edit, and its only purpose is to be
/// [`merge`](crate::Replica::merge)d by all the other
/// [`Replica`](crate::Replica)s in the same editing session to create
/// [`TextEdit`]s, which can then be applied to their local text buffers.
///
/// See the the documentation of any of the methods mentioned above for more
/// information.
#[derive(Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CrdtEdit {
    #[cfg_attr(feature = "serde", serde(flatten))]
    kind: CrdtEditKind,
}

impl core::fmt::Debug for CrdtEdit {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self.kind() {
            CrdtEditKind::Deletion(deletion) => deletion.fmt(f),
            CrdtEditKind::Insertion(insertion) => insertion.fmt(f),
            CrdtEditKind::NoOp => f.write_str("NoOp"),
        }
    }
}

impl CrdtEdit {
    #[inline]
    pub(crate) fn _deletion(
        start: Anchor,
        start_ts: RunTs,
        end: Anchor,
        end_ts: RunTs,
        version_map: VersionMap,
        deletion_ts: DeletionTs,
    ) -> Self {
        let deletion = Deletion {
            start,
            start_ts,
            end,
            end_ts,
            version_map,
            deletion_ts,
        };
        Self { kind: CrdtEditKind::Deletion(deletion) }
    }

    #[inline]
    pub(crate) fn _insertion(
        anchor: Anchor,
        anchor_ts: RunTs,
        text: Text,
        lamport_ts: LamportTs,
        run_ts: RunTs,
    ) -> Self {
        let insertion =
            Insertion { anchor, anchor_ts, text, lamport_ts, run_ts };
        Self { kind: CrdtEditKind::Insertion(insertion) }
    }

    #[inline(always)]
    pub(crate) fn kind(&self) -> &CrdtEditKind {
        &self.kind
    }

    #[inline]
    pub(crate) fn _no_op() -> Self {
        Self { kind: CrdtEditKind::NoOp }
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[allow(dead_code)]
pub(crate) enum CrdtEditKind {
    Deletion(Deletion),
    Insertion(Insertion),
    NoOp,
}

/// An insertion in CRDT coordinates.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    any(feature = "encode", feature = "serde"),
    derive(serde::Serialize, serde::Deserialize)
)]
pub struct Insertion {
    /// The anchor point of the insertion.
    anchor: Anchor,

    /// The run timestamp of the [`EditRun`] containing the anchor.
    anchor_ts: RunTs,

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
    pub(crate) fn anchor_ts(&self) -> RunTs {
        self.anchor_ts
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
        anchor_ts: RunTs,
        text: Text,
        lamport_ts: LamportTs,
        run_ts: RunTs,
    ) -> Self {
        Self { anchor, anchor_ts, text, lamport_ts, run_ts }
    }

    #[inline]
    pub(crate) fn no_op() -> Self {
        Self::new(
            Anchor::origin(),
            0,
            Text::new(ReplicaId::zero(), 0..0),
            0,
            0,
        )
    }

    #[inline]
    pub(crate) fn start(&self) -> Length {
        self.text.range.start
    }

    /// Returns the [`Text`] that was inserted.
    #[inline]
    pub fn text(&self) -> &Text {
        &self.text
    }
}

/// A deletion in CRDT coordinates.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    any(feature = "encode", feature = "serde"),
    derive(serde::Serialize, serde::Deserialize)
)]
pub struct Deletion {
    /// The anchor point of the start of the deleted range.
    start: Anchor,

    /// The run timestamp of the [`EditRun`] containing the start `Anchor`.
    start_ts: RunTs,

    /// The anchor point of the end of the deleted range.
    end: Anchor,

    /// The run timestamp of the [`EditRun`] containing the end `Anchor`.
    end_ts: RunTs,

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

    #[inline(always)]
    pub(crate) fn end_ts(&self) -> RunTs {
        self.end_ts
    }

    #[inline]
    pub(crate) fn is_no_op(&self) -> bool {
        self.end.is_zero()
    }

    #[inline]
    pub(crate) fn new(
        start: Anchor,
        start_ts: RunTs,
        end: Anchor,
        end_ts: RunTs,
        version_map: VersionMap,
        deletion_ts: DeletionTs,
    ) -> Self {
        Deletion { start, start_ts, end, end_ts, version_map, deletion_ts }
    }

    #[inline]
    pub(crate) fn no_op() -> Self {
        Self::new(
            Anchor::origin(),
            0,
            Anchor::origin(),
            0,
            VersionMap::new(ReplicaId::zero(), 0),
            0,
        )
    }

    #[inline(always)]
    pub(crate) fn start(&self) -> Anchor {
        self.start
    }

    #[inline(always)]
    pub(crate) fn start_ts(&self) -> RunTs {
        self.start_ts
    }

    #[inline(always)]
    pub(crate) fn version_map(&self) -> &VersionMap {
        &self.version_map
    }
}
