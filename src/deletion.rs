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
    /// Returns the [`ReplicaId`] of the [`Replica`] that performed the
    /// deletion.
    ///
    /// # Examples
    ///
    /// ```
    /// # use cola::Replica;
    /// let mut replica1 = Replica::new(1, 10);
    ///
    /// let deletion = replica1.deleted(3..7);
    ///
    /// assert_eq!(deletion.deleted_by(), replica1.id());
    /// ```
    #[inline(always)]
    pub fn deleted_by(&self) -> ReplicaId {
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
        start: &'a InnerAnchor,
        end: &'a InnerAnchor,
    }

    impl<'a> Anchors<'a> {
        #[inline]
        fn new(start: &'a InnerAnchor, end: &'a InnerAnchor) -> Self {
            Self { start, end }
        }
    }

    impl Encode for Anchors<'_> {
        #[inline]
        fn encode(&self, buf: &mut Vec<u8>) {
            let flag = AnchorsFlag::from_anchors(self);

            flag.encode(buf);

            match flag {
                AnchorsFlag::DifferentReplicaIds => {
                    self.start.encode(buf);
                    self.end.encode(buf);
                },
                AnchorsFlag::SameReplicaId => {
                    self.start.replica_id().encode(buf);
                    self.start.run_ts().encode(buf);
                    self.start.offset().encode(buf);
                    self.end.run_ts().encode(buf);
                    self.end.offset().encode(buf);
                },
                AnchorsFlag::SameReplicaIdAndRunTs => {
                    self.start.replica_id().encode(buf);
                    self.start.run_ts().encode(buf);
                    self.start.offset().encode(buf);
                    // The start and end are on the same run, so we know that
                    // the end has a greater offset than the start.
                    let length = self.end.offset() - self.start.offset();
                    length.encode(buf);
                },
            }
        }
    }

    pub(crate) enum AnchorsDecodeError {
        Int(IntDecodeError),
        Flag(AnchorsFlagDecodeError),
    }

    impl From<IntDecodeError> for AnchorsDecodeError {
        #[inline(always)]
        fn from(err: IntDecodeError) -> Self {
            Self::Int(err)
        }
    }

    impl From<AnchorsFlagDecodeError> for AnchorsDecodeError {
        #[inline(always)]
        fn from(err: AnchorsFlagDecodeError) -> Self {
            Self::Flag(err)
        }
    }

    impl core::fmt::Display for AnchorsDecodeError {
        #[inline]
        fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
            let err: &dyn core::fmt::Display = match self {
                Self::Int(err) => err,
                Self::Flag(err) => err,
            };

            write!(f, "Anchors couldn't be decoded: {err}")
        }
    }

    impl Decode for Anchors<'_> {
        type Value = (Anchor, Anchor);

        type Error = AnchorsDecodeError;

        #[inline]
        fn decode(buf: &[u8]) -> Result<(Self::Value, &[u8]), Self::Error> {
            let (flag, buf) = AnchorsFlag::decode(buf)?;

            match flag {
                AnchorsFlag::DifferentReplicaIds => {
                    let (start, buf) = InnerAnchor::decode(buf)?;
                    let (end, buf) = InnerAnchor::decode(buf)?;
                    Ok(((start, end), buf))
                },
                AnchorsFlag::SameReplicaId => {
                    let (replica_id, buf) = ReplicaId::decode(buf)?;
                    let (start_run_ts, buf) = RunTs::decode(buf)?;
                    let (start_offset, buf) = Length::decode(buf)?;
                    let (end_run_ts, buf) = RunTs::decode(buf)?;
                    let (end_offset, buf) = Length::decode(buf)?;

                    let start = InnerAnchor::new(
                        replica_id,
                        start_offset,
                        start_run_ts,
                    );

                    let end =
                        InnerAnchor::new(replica_id, end_offset, end_run_ts);

                    Ok(((start, end), buf))
                },
                AnchorsFlag::SameReplicaIdAndRunTs => {
                    let (replica_id, buf) = ReplicaId::decode(buf)?;
                    let (run_ts, buf) = RunTs::decode(buf)?;
                    let (offset, buf) = Length::decode(buf)?;
                    let (length, buf) = Length::decode(buf)?;
                    let start = InnerAnchor::new(replica_id, offset, run_ts);
                    let end =
                        InnerAnchor::new(replica_id, offset + length, run_ts);
                    Ok(((start, end), buf))
                },
            }
        }
    }

    enum AnchorsFlag {
        SameReplicaId,
        SameReplicaIdAndRunTs,
        DifferentReplicaIds,
    }

    impl AnchorsFlag {
        #[inline(always)]
        fn from_anchors(anchors: &Anchors) -> Self {
            if anchors.start.replica_id() == anchors.end.replica_id() {
                if anchors.start.run_ts() == anchors.end.run_ts() {
                    Self::SameReplicaIdAndRunTs
                } else {
                    Self::SameReplicaId
                }
            } else {
                Self::DifferentReplicaIds
            }
        }
    }

    impl Encode for AnchorsFlag {
        #[inline]
        fn encode(&self, buf: &mut Vec<u8>) {
            let flag: u8 = match self {
                Self::DifferentReplicaIds => 0,
                Self::SameReplicaId => 1,
                Self::SameReplicaIdAndRunTs => 2,
            };

            buf.push(flag);
        }
    }

    pub(crate) enum AnchorsFlagDecodeError {
        EmptyBuffer,
        InvalidByte(u8),
    }

    impl core::fmt::Display for AnchorsFlagDecodeError {
        #[inline]
        fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
            match self {
                Self::EmptyBuffer => write!(
                    f,
                    "AnchorsFlag can't be decoded from an empty buffer"
                ),
                Self::InvalidByte(byte) => write!(
                    f,
                    "AnchorsFlag can't be decoded from {}, it must be 0, 1, \
                     or 2",
                    byte
                ),
            }
        }
    }

    impl Decode for AnchorsFlag {
        type Value = Self;

        type Error = AnchorsFlagDecodeError;

        #[inline]
        fn decode(buf: &[u8]) -> Result<(Self::Value, &[u8]), Self::Error> {
            let (&flag, buf) = buf
                .split_first()
                .ok_or(AnchorsFlagDecodeError::EmptyBuffer)?;

            let this = match flag {
                0 => Self::DifferentReplicaIds,
                1 => Self::SameReplicaId,
                2 => Self::SameReplicaIdAndRunTs,
                _ => return Err(AnchorsFlagDecodeError::InvalidByte(flag)),
            };

            Ok((this, buf))
        }
    }
}

#[cfg(feature = "serde")]
mod serde {
    crate::encode::impl_serialize!(super::Deletion);
    crate::encode::impl_deserialize!(super::Deletion);
}
