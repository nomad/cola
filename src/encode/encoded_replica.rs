use crate::*;

/// TODO: docs
#[derive(Clone, PartialEq, Eq)]
pub struct EncodedReplica {
    protocol_version: ProtocolVersion,
    checksum: (),
    bytes: Vec<u8>,
}

/// TODO: docs
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// TODO: docs
    Checksum {
        /// TODO: docs
        expected: (),
        /// TODO: docs
        actual: (),
    },

    /// TODO: docs
    ProtocolVersion {
        /// TODO: docs
        encoded_on: ProtocolVersion,
        /// TODO: docs
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
