use core::fmt::Display;

/// TODO: docs
pub(crate) trait Encode {
    /// TODO: docs
    fn encode(&self, buf: &mut Vec<u8>);
}

/// TODO: docs
pub(crate) trait Decode {
    type Value: Sized;

    type Error: Display;

    /// TODO: docs
    fn decode(buf: &[u8]) -> Result<(Self::Value, &[u8]), Self::Error>;
}

/// TODO: docs
pub(crate) trait DecodeWithCtx {
    type Value: Sized;

    type Error: Display;

    type Ctx;

    /// TODO: docs
    fn decode<'buf>(
        buf: &'buf [u8],
        ctx: &mut Self::Ctx,
    ) -> Result<(Self::Value, &'buf [u8]), Self::Error>;
}

impl Encode for bool {
    #[inline]
    fn encode(&self, buf: &mut Vec<u8>) {
        buf.push(*self as u8);
    }
}

impl Decode for bool {
    type Value = bool;

    type Error = BoolDecodeError;

    #[inline]
    fn decode(buf: &[u8]) -> Result<(bool, &[u8]), Self::Error> {
        let (&byte, buf) =
            buf.split_first().ok_or(BoolDecodeError::EmptyBuffer)?;

        match byte {
            0 => Ok((false, buf)),
            1 => Ok((true, buf)),
            _ => Err(BoolDecodeError::InvalidByte(byte)),
        }
    }
}

pub(crate) enum BoolDecodeError {
    EmptyBuffer,
    InvalidByte(u8),
}

impl core::fmt::Display for BoolDecodeError {
    #[inline]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::EmptyBuffer => f.write_str(
                "bool couldn't be decoded because the buffer is empty",
            ),
            Self::InvalidByte(byte) => {
                write!(
                    f,
                    "bool cannot be decoded from byte {}, it must be 0 or 1",
                    byte,
                )
            },
        }
    }
}

// When encoding integers, we:
//
// - turn the integer into the corresponding little-endian byte array. The
//   resulting array will have a fixed length equal to `mem::size_of::<I>`;
//
// - ignore any trailing zeroes in the resulting byte array;
//
// - push the length of the resulting byte slice;
//
// - push the byte slice;
//
// For example, `256u64` gets encoded as `[2, 0, 1]`.
//
// With this scheme we could potentially encode integers up to
// `2 ^ (255 * 8) - 1`, which is ridiculously overkill for our use case since
// we only need to encode integers up to `u64::MAX`.
//
// Because of this, we actually use the first byte to encode the integer
// itself if it's either 0 or between 9 and 255. We don't do this for 1..=8
// because we need to reserve those to represent the number of bytes that
// follow.
//
// A few examples:
//
// - `0` is encoded as `[0]`;
//
// - `1` is encoded as `[1, 1]`;
//
// - `8` is encoded as `[1, 8]`;
//
// - `9` is encoded as `[9]`;
//
// - `255` is encoded as `[255]`;
//
// - numbers greater than 255 are always encoded as `[length, ..bytes..]`.

/// TODO: docs
const ENCODE_ONE_BYTE_MASK: u8 = 0b0100_0000;

/// TODO: docs
const ENCODE_TWO_BYTES_MASK: u8 = 0b1000_0000;

#[inline(always)]
fn encode_one_byte(int: u8) -> u8 {
    debug_assert!(int < 1 << 6);
    int | ENCODE_ONE_BYTE_MASK
}

#[inline(always)]
fn decode_one_byte(int: u8) -> u8 {
    int & !ENCODE_ONE_BYTE_MASK
}

#[inline(always)]
fn encode_two_bytes(int: u16) -> (u8, u8) {
    debug_assert!((1 << 6..1 << 15).contains(&int));

    let [mut lo, mut hi] = int.to_le_bytes();

    // Move the last bit of the low byte to the last bit of the high byte.
    //
    // We know this doesn't lose any information because the int is less than
    // 2 ^ 15, so the last bit of the high byte is 0.
    hi |= lo & 0b1000_0000;

    // Set the last bit of the low byte to 1 to indicate that this number is
    // encoded with 2 bytes.
    lo |= ENCODE_TWO_BYTES_MASK;

    (lo, hi)
}

#[inline(always)]
fn decode_two_bytes(mut lo: u8, mut hi: u8) -> u16 {
    lo &= !ENCODE_TWO_BYTES_MASK;

    // Move the last bit of the high byte to the last bit of the low byte.
    lo |= hi & 0b1000_0000;

    // Reset the last bit of the high byte to 0.
    hi &= !0b1000_0000;

    u16::from_le_bytes([lo, hi])
}

impl_int_encode!(u16);
impl_int_encode!(u32);
impl_int_encode!(u64);

impl_int_decode!(u16);
impl_int_decode!(u32);
impl_int_decode!(u64);

impl Encode for usize {
    #[inline(always)]
    fn encode(&self, buf: &mut Vec<u8>) {
        (*self as u64).encode(buf)
    }
}

impl Decode for usize {
    type Value = usize;

    type Error = IntDecodeError;

    #[inline(always)]
    fn decode(buf: &[u8]) -> Result<(usize, &[u8]), Self::Error> {
        u64::decode(buf).map(|(value, rest)| (value as usize, rest))
    }
}

macro_rules! impl_int_encode {
    ($ty:ty) => {
        impl Encode for $ty {
            #[inline]
            fn encode(&self, buf: &mut Vec<u8>) {
                let int = *self;

                if int < 1 << 6 {
                    buf.push(encode_one_byte(int as u8));
                    return;
                } else if int < 1 << 15 {
                    let (first, second) = encode_two_bytes(int as u16);
                    buf.push(first);
                    buf.push(second);
                    return;
                }

                let array = int.to_le_bytes();

                let num_trailing_zeros = array
                    .iter()
                    .rev()
                    .copied()
                    .take_while(|&byte| byte == 0)
                    .count();

                let len = array.len() - num_trailing_zeros;

                // Make sure that the first 2 bits are 0.
                debug_assert_eq!(len & 0b1100_0000, 0);

                buf.push(len as u8);

                buf.extend_from_slice(&array[..len]);
            }
        }
    };
}

use impl_int_encode;

macro_rules! impl_int_decode {
    ($ty:ty) => {
        impl Decode for $ty {
            type Value = Self;

            type Error = $crate::encode::IntDecodeError;

            #[inline]
            fn decode(buf: &[u8]) -> Result<($ty, &[u8]), Self::Error> {
                let (&first, buf) =
                    buf.split_first().ok_or(IntDecodeError::EmptyBuffer)?;

                if first & ENCODE_TWO_BYTES_MASK != 0 {
                    let lo = first;

                    let (&hi, buf) = buf.split_first().ok_or(
                        IntDecodeError::LengthLessThanPrefix {
                            prefix: 2,
                            actual: 1,
                        },
                    )?;

                    let int = decode_two_bytes(lo, hi) as $ty;

                    return Ok((int, buf));
                } else if first & ENCODE_ONE_BYTE_MASK != 0 {
                    let int = decode_one_byte(first) as $ty;
                    return Ok((int, buf));
                }

                let len = first;

                if len as usize > buf.len() {
                    return Err(IntDecodeError::LengthLessThanPrefix {
                        prefix: len,
                        actual: buf.len() as u8,
                    });
                }

                let mut array = [0u8; ::core::mem::size_of::<$ty>()];

                let (bytes, buf) = buf.split_at(len as usize);

                array[..bytes.len()].copy_from_slice(bytes);

                let int = <$ty>::from_le_bytes(array);

                Ok((int, buf))
            }
        }
    };
}

use impl_int_decode;

/// An error that can occur when decoding an [`Int`].
#[cfg_attr(test, derive(PartialEq, Eq))]
pub(crate) enum IntDecodeError {
    /// The buffer passed to `Int::decode` is empty. This is always an error,
    /// even if the integer being decoded is zero.
    EmptyBuffer,

    /// The actual byte length of the buffer is less than what was specified
    /// in the prefix.
    LengthLessThanPrefix { prefix: u8, actual: u8 },
}

impl Display for IntDecodeError {
    #[inline]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::EmptyBuffer => f.write_str(
                "Int couldn't be decoded because the buffer is empty",
            ),
            Self::LengthLessThanPrefix { prefix, actual } => {
                write!(
                    f,
                    "Int couldn't be decoded because the buffer's length is \
                     {actual}, but the prefix specified a length of {prefix}",
                )
            },
        }
    }
}

#[cfg(feature = "serde")]
pub(crate) use serde::{impl_deserialize, impl_serialize};

#[cfg(feature = "serde")]
mod serde {
    macro_rules! impl_deserialize {
        ($ty:ty) => {
            impl<'de> ::serde::de::Deserialize<'de> for $ty {
                #[inline]
                fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
                where
                    D: ::serde::de::Deserializer<'de>,
                {
                    struct Visitor;

                    impl<'de> ::serde::de::Visitor<'de> for Visitor {
                        type Value = <$ty as $crate::encode::Decode>::Value;

                        #[inline]
                        fn expecting(
                            &self,
                            formatter: &mut ::core::fmt::Formatter,
                        ) -> ::core::fmt::Result {
                            formatter.write_str("a byte slice")
                        }

                        #[inline]
                        fn visit_bytes<E>(
                            self,
                            v: &[u8],
                        ) -> Result<Self::Value, E>
                        where
                            E: ::serde::de::Error,
                        {
                            <Self::Value as $crate::encode::Decode>::decode(v)
                                .map(|(value, _rest)| value)
                                .map_err(E::custom)
                        }

                        #[inline]
                        fn visit_seq<A>(
                            self,
                            mut seq: A,
                        ) -> Result<Self::Value, A::Error>
                        where
                            A: ::serde::de::SeqAccess<'de>,
                        {
                            let size = seq.size_hint().unwrap_or(0);
                            let mut buf =
                                ::alloc::vec::Vec::<u8>::with_capacity(size);
                            while let Some(byte) = seq.next_element()? {
                                buf.push(byte);
                            }
                            <Self::Value as $crate::encode::Decode>::decode(
                                &buf,
                            )
                            .map(|(value, _rest)| value)
                            .map_err(<A::Error as ::serde::de::Error>::custom)
                        }
                    }

                    deserializer.deserialize_bytes(Visitor)
                }
            }
        };
    }

    macro_rules! impl_serialize {
        ($ty:ty) => {
            impl ::serde::ser::Serialize for $ty {
                #[inline]
                fn serialize<S>(
                    &self,
                    serializer: S,
                ) -> Result<S::Ok, S::Error>
                where
                    S: ::serde::ser::Serializer,
                {
                    let mut buf = Vec::new();
                    <Self as $crate::encode::Encode>::encode(&self, &mut buf);
                    serializer.serialize_bytes(&buf)
                }
            }
        };
    }

    pub(crate) use impl_deserialize;
    pub(crate) use impl_serialize;
}

#[cfg(test)]
mod tests {
    use super::*;

    impl core::fmt::Debug for IntDecodeError {
        #[inline]
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            core::fmt::Display::fmt(self, f)
        }
    }

    /// Tests that some integers can be encoded with a single byte.
    #[test]
    fn encode_int_single_byte() {
        let mut buf = Vec::new();

        for int in 0..1 << 6 {
            int.encode(&mut buf);
            assert_eq!(buf.len(), 1);
            let (decoded, rest) = u64::decode(&buf).unwrap();
            assert_eq!(int, decoded);
            assert!(rest.is_empty());
            buf.clear();
        }
    }

    /// Tests that integers are encoded using the correct number of bytes.
    #[test]
    fn encode_int_num_bytes() {
        fn expected_len(int: u64) -> u8 {
            if int < 1 << 6 {
                1
            } else if int < 1 << 15 {
                2
            } else if int < 1 << 16 {
                3
            } else if int < 1 << 24 {
                4
            } else if int < 1 << 32 {
                5
            } else if int < 1 << 40 {
                6
            } else if int < 1 << 48 {
                7
            } else if int < 1 << 56 {
                8
            } else {
                9
            }
        }

        let ints = (1..=8).chain([
            0,
            (1 << 6) - 1,
            1 << 6,
            (1 << 6) + 1,
            (1 << 15) - 1,
            1 << 15,
            (1 << 15) + 1,
            u16::MAX as u64 - 1,
            u16::MAX as u64,
            u16::MAX as u64 + 1,
            u32::MAX as u64,
            u32::MAX as u64 + 1,
            u64::MAX,
        ]);

        let mut buf = Vec::new();

        for int in ints {
            int.encode(&mut buf);

            assert_eq!(buf.len(), expected_len(int) as usize);

            let (decoded, rest) = u64::decode(&buf).unwrap();

            assert_eq!(int, decoded);

            assert!(rest.is_empty());

            buf.clear();
        }
    }

    /// Tests that decoding an integer fails if the buffer is empty.
    #[test]
    fn encode_int_fails_if_buffer_empty() {
        let mut buf = Vec::new();

        42u32.encode(&mut buf);

        buf.clear();

        assert_eq!(
            u32::decode(&buf).unwrap_err(),
            IntDecodeError::EmptyBuffer
        );
    }

    /// Tests that decoding an integer fails if the length specified in the
    /// prefix is greater than the actual length of the buffer.
    #[test]
    fn encode_int_fails_if_buffer_too_short() {
        let mut buf = Vec::new();

        u16::MAX.encode(&mut buf);

        buf.pop();

        assert_eq!(
            u32::decode(&buf).unwrap_err(),
            IntDecodeError::LengthLessThanPrefix { prefix: 2, actual: 1 }
        );
    }
}
