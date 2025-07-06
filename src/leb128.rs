use core::fmt;

macro_rules! encode {
    ($fn_name:ident, $ty:ident, $max_encoded_bytes:expr) => {
        #[inline]
        pub(crate) fn $fn_name(value: $ty) -> ([u8; $max_encoded_bytes], u8) {
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            {
                let (long_array, len_u8) = varint_simd::encode(value);
                let mut short_array = [0; $max_encoded_bytes];
                let len = len_u8 as usize;
                short_array[..len].copy_from_slice(&long_array[..len]);
                (short_array, len_u8)
            }
            #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
            {
                let mut buf = [0; $max_encoded_bytes];
                let len = unsigned_varint::encode::$ty(value, &mut buf).len();
                (buf, len as u8)
            }
        }
    };
}

macro_rules! decode {
    ($fn_name:ident, $ty:ident) => {
        #[inline]
        pub(crate) fn $fn_name(buf: &[u8]) -> Result<($ty, u8), DecodeError> {
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            {
                let (decoded, len) = varint_simd::decode(buf)?;
                // TODO: this check shouldn't be necessary, `decode` should
                // fail. Open an issue.
                if buf.len() < len {
                    return Err(DecodeError::NotEnoughBytes);
                }
                Ok((decoded, len as u8))
            }
            #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
            {
                let (decoded, rest) = unsigned_varint::decode::$ty(buf)?;
                Ok((decoded, (buf.len() - rest.len()) as u8))
            }
        }
    };
}

encode!(encode_u16, u16, 3);
encode!(encode_u32, u32, 5);
encode!(encode_u64, u64, 10);

decode!(decode_u16, u16);
decode!(decode_u32, u32);
decode!(decode_u64, u64);

#[cfg_attr(test, derive(Debug, PartialEq))]
pub(crate) enum DecodeError {
    NotEnoughBytes,
    #[cfg_attr(
        any(target_arch = "x86", target_arch = "x86_64"),
        allow(dead_code)
    )]
    NotMinimal,
    Overflow,
}

impl fmt::Display for DecodeError {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotEnoughBytes => f.write_str("not enough input bytes"),
            Self::NotMinimal => f.write_str("encoding is not minimal"),
            Self::Overflow => f.write_str("input bytes exceed maximum"),
        }
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
impl From<varint_simd::VarIntDecodeError> for DecodeError {
    #[inline]
    fn from(err: varint_simd::VarIntDecodeError) -> Self {
        match err {
            varint_simd::VarIntDecodeError::Overflow => Self::Overflow,
            varint_simd::VarIntDecodeError::NotEnoughBytes => {
                Self::NotEnoughBytes
            },
        }
    }
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
impl From<unsigned_varint::decode::Error> for DecodeError {
    #[inline]
    fn from(err: unsigned_varint::decode::Error) -> Self {
        match err {
            unsigned_varint::decode::Error::Insufficient => {
                Self::NotEnoughBytes
            },
            unsigned_varint::decode::Error::Overflow => Self::Overflow,
            unsigned_varint::decode::Error::NotMinimal => Self::NotMinimal,
            _ => unimplemented!(),
        }
    }
}
