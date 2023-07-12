use crate::*;

/// TODO: docs
#[derive(Clone, PartialEq, Eq)]
pub struct EncodedReplica {
    protocol_version: ProtocolVersion,
    checksum: (),
    bytes: Vec<u8>,
}

/// The type of error that can occur when [`decode`](Replica::decode)ing an
/// [`EncodedReplica`].
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
}

#[cfg(feature = "serde")]
mod serde {
    use ::serde::{de, ser};

    use super::*;

    impl ser::Serialize for EncodedReplica {
        #[inline]
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: ser::Serializer,
        {
            todo!();
        }
    }

    impl<'de> de::Deserialize<'de> for EncodedReplica {
        #[inline]
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: de::Deserializer<'de>,
        {
            todo!();
        }
    }
}
