use crate::anchor::InnerAnchor as Anchor;
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
