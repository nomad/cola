use std::error::Error as StdError;

/// TODO: docs
pub(crate) trait Encode {
    /// TODO: docs
    fn encode(&self, buf: &mut Vec<u8>);
}

/// TODO: docs
pub(crate) trait Decode: Sized {
    type Error: StdError;

    /// TODO: docs
    fn decode(buf: &[u8]) -> Result<(Self, &[u8]), Self::Error>;
}
