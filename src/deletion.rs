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

#[cfg(feature = "encode")]
mod encode {
    use super::*;
    use crate::encode::{Decode, Encode, IntDecodeError};
    use crate::version_map::encode::BaseMapDecodeError;

    impl Encode for Deletion {
        #[inline]
        fn encode(&self, buf: &mut Vec<u8>) {
            Anchors::new(&self.start, &self.end).encode(buf);
            self.version_map.encode(buf);
            self.deletion_ts.encode(buf);
        }
    }

    pub(crate) enum DeletionDecodeError {
        Anchors(AnchorsDecodeError),
        Int(IntDecodeError),
        VersionMap(BaseMapDecodeError<Length>),
    }

    impl From<AnchorsDecodeError> for DeletionDecodeError {
        #[inline(always)]
        fn from(err: AnchorsDecodeError) -> Self {
            Self::Anchors(err)
        }
    }

    impl From<IntDecodeError> for DeletionDecodeError {
        #[inline(always)]
        fn from(err: IntDecodeError) -> Self {
            Self::Int(err)
        }
    }

    impl From<BaseMapDecodeError<Length>> for DeletionDecodeError {
        #[inline(always)]
        fn from(err: BaseMapDecodeError<Length>) -> Self {
            Self::VersionMap(err)
        }
    }

    impl core::fmt::Display for DeletionDecodeError {
        #[inline]
        fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
            let err: &dyn core::fmt::Display = match self {
                Self::Anchors(err) => err,
                Self::Int(err) => err,
                Self::VersionMap(err) => err,
            };

            write!(f, "Deletion couldn't be decoded: {err}")
        }
    }

    impl Decode for Deletion {
        type Value = Self;

        type Error = DeletionDecodeError;

        #[inline]
        fn decode(buf: &[u8]) -> Result<(Self, &[u8]), Self::Error> {
            let ((start, end), buf) = Anchors::decode(buf)?;
            let (version_map, buf) = VersionMap::decode(buf)?;
            let (deletion_ts, buf) = DeletionTs::decode(buf)?;
            let this = Self::new(start, end, version_map, deletion_ts);
            Ok((this, buf))
        }
    }

    /// TODO: docs
    #[allow(dead_code)]
    struct Anchors<'a> {
        start: &'a Anchor,
        end: &'a Anchor,
    }

    impl<'a> Anchors<'a> {
        #[inline]
        fn new(start: &'a Anchor, end: &'a Anchor) -> Self {
            Self { start, end }
        }
    }

    impl Encode for Anchors<'_> {
        #[inline]
        fn encode(&self, _buf: &mut Vec<u8>) {}
    }

    pub(crate) enum AnchorsDecodeError {}

    impl core::fmt::Display for AnchorsDecodeError {
        #[inline]
        fn fmt(&self, _f: &mut core::fmt::Formatter) -> core::fmt::Result {
            todo!()
        }
    }

    impl Decode for Anchors<'_> {
        type Value = (Anchor, Anchor);

        type Error = AnchorsDecodeError;

        #[inline]
        fn decode(_buf: &[u8]) -> Result<(Self::Value, &[u8]), Self::Error> {
            todo!();
        }
    }
}
