use core::fmt;
use core::ops::Deref;

use sha2::{Digest, Sha256};

use crate::encode::{Decode, Encode};
use crate::*;

type Checksum = [u8; 32];

const CHECKSUM_LEN: usize = core::mem::size_of::<Checksum>();

/// A [`Replica`] encoded into a compact binary format suitable for
/// transmission over the network.
///
/// This struct is created by [`encode`](Replica::encode)ing a [`Replica`] and
/// can be decoded back into a [`Replica`] by calling
/// [`decode`](Replica::decode). See the documentation of those methods for
/// more information.
#[cfg_attr(docsrs, doc(cfg(feature = "encode")))]
#[derive(Clone)]
pub struct EncodedReplica<'buf> {
    bytes: Bytes<'buf>,
}

#[derive(Clone)]
enum Bytes<'a> {
    Owned(Box<[u8]>),
    Borrowed(&'a [u8]),
}

impl<'buf> EncodedReplica<'buf> {
    /// Returns the raw bytes of the encoded replica.
    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Creates an `EncodedReplica` from the given bytes.
    #[inline]
    pub fn from_bytes(bytes: &'buf [u8]) -> Self {
        bytes.into()
    }

    /// Copies the underlying bytes into a new `EncodedReplica` with a static
    /// lifetime.
    #[inline]
    pub fn to_static(&self) -> EncodedReplica<'static> {
        EncodedReplica {
            bytes: match &self.bytes {
                Bytes::Owned(bytes) => Bytes::Owned(bytes.clone()),
                Bytes::Borrowed(bytes) => Bytes::Owned((*bytes).into()),
            },
        }
    }

    #[inline]
    pub(crate) fn to_replica(
        &self,
    ) -> Result<<Replica as Decode>::Value, DecodeError> {
        let bytes = &*self.bytes;

        let (protocol_version, buf) = ProtocolVersion::decode(bytes)
            .map_err(|_| DecodeError::InvalidData)?;

        if protocol_version != crate::PROTOCOL_VERSION {
            return Err(DecodeError::DifferentProtocol {
                encoded_on: protocol_version,
                decoding_on: crate::PROTOCOL_VERSION,
            });
        }

        if buf.len() < CHECKSUM_LEN {
            return Err(DecodeError::InvalidData);
        }

        let (checksum_slice, buf) = buf.split_at(CHECKSUM_LEN);

        if checksum_slice != checksum(buf) {
            return Err(DecodeError::ChecksumFailed);
        }

        <Replica as Decode>::decode(buf)
            .map(|(value, _rest)| value)
            .map_err(|_| DecodeError::InvalidData)
    }
}

impl EncodedReplica<'static> {
    #[inline]
    pub(crate) fn from_replica(replica: &Replica) -> Self {
        let mut bytes = Vec::new();
        crate::PROTOCOL_VERSION.encode(&mut bytes);
        let protocol_len = bytes.len();
        let dummy_checksum = Checksum::default();
        bytes.extend_from_slice(&dummy_checksum);
        Encode::encode(replica, &mut bytes);
        let replica_start = protocol_len + CHECKSUM_LEN;
        let checksum = checksum(&bytes[replica_start..]);
        bytes[protocol_len..protocol_len + CHECKSUM_LEN]
            .copy_from_slice(&checksum);
        Self { bytes: Bytes::Owned(bytes.into()) }
    }
}

impl fmt::Debug for EncodedReplica<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EncodedReplica").finish_non_exhaustive()
    }
}

impl<'buf> From<&'buf [u8]> for EncodedReplica<'buf> {
    #[inline]
    fn from(bytes: &'buf [u8]) -> Self {
        Self { bytes: Bytes::Borrowed(bytes) }
    }
}

impl From<Box<[u8]>> for EncodedReplica<'static> {
    #[inline]
    fn from(bytes: Box<[u8]>) -> Self {
        Self { bytes: Bytes::Owned(bytes) }
    }
}

impl AsRef<[u8]> for EncodedReplica<'_> {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        &*self.bytes
    }
}

impl Deref for EncodedReplica<'_> {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &Self::Target {
        &*self.bytes
    }
}

impl PartialEq<Self> for EncodedReplica<'_> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        *self.bytes == *other.bytes
    }
}

impl Eq for EncodedReplica<'_> {}

impl Encode for EncodedReplica<'_> {
    #[inline]
    fn encode(&self, buf: &mut Vec<u8>) {
        debug_assert!(buf.is_empty());
        buf.extend_from_slice(&*self.bytes);
    }
}

impl Decode for EncodedReplica<'static> {
    type Value = Self;
    type Error = core::convert::Infallible;

    #[inline]
    fn decode(buf: &[u8]) -> Result<(Self, &[u8]), Self::Error> {
        Ok((EncodedReplica::from_bytes(buf).to_static(), &[]))
    }
}

impl Deref for Bytes<'_> {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &Self::Target {
        match self {
            Bytes::Owned(bytes) => bytes,
            Bytes::Borrowed(bytes) => bytes,
        }
    }
}

/// The type of error that can occur when [`decode`](Replica::decode)ing an
/// [`EncodedReplica`].
#[cfg_attr(docsrs, doc(cfg(feature = "encode")))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// This error occurs when the internal checksum of the [`EncodedReplica`]
    /// fails.
    ///
    /// This typically means that the [`EncodedReplica`] was corrupted during
    /// transmission.
    ChecksumFailed,

    /// This error occurs when the machine that created the [`EncodedReplica`]
    /// and the one that is trying to [`decode`](Replica::decode) it are using
    /// two incompatible versions of cola.
    DifferentProtocol {
        /// The `ProtocolVersion` of cola on the machine that created the
        /// `EncodedReplica`.
        encoded_on: ProtocolVersion,

        /// The `ProtocolVersion` of cola on the machine that is trying to
        /// decode the `EncodedReplica`.
        decoding_on: ProtocolVersion,
    },

    /// This error is an umbrella variant that encompasses all other errors
    /// that can occur when the binary data wrapped by the [`EncodedReplica`]
    /// cannot be decoded into a `Replica`.
    ///
    /// This is returned when the checksum and protocol version checks both
    /// succeed, *and yet* the data is still invalid. The only way this can
    /// occur in practice is if the `EncodedReplica` passed to
    /// [`decode`](Replica::decode) was deserialized from a byte vector that
    /// was not the result of serializing an `EncodedReplica`.
    ///
    /// As long as you're not doing that (and you shouldn't be) this variant
    /// can be ignored.
    InvalidData,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::ChecksumFailed => f.write_str("checksum failed"),
            DecodeError::DifferentProtocol { encoded_on, decoding_on } => {
                write!(
                    f,
                    "different protocol: encoded on {encoded_on:?}, decoding \
                     on {decoding_on:?}",
                )
            },
            DecodeError::InvalidData => f.write_str("invalid data"),
        }
    }
}

impl std::error::Error for DecodeError {}

#[inline(always)]
pub(crate) fn checksum(bytes: &[u8]) -> Checksum {
    let checksum = Sha256::digest(bytes);
    *checksum.as_ref()
}

#[cfg(feature = "serde")]
mod serde {
    crate::encode::impl_serialize!(super::EncodedReplica<'_>);
    crate::encode::impl_deserialize!(super::EncodedReplica<'static>);
}
