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

/// A variable-length encoded integer.
pub(crate) struct Int<I>(I);

impl<I> Int<I> {
    #[inline]
    pub(crate) fn new(integer: I) -> Self {
        Self(integer)
    }
}

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

impl_int_encode!(u8);
impl_int_encode!(u16);
impl_int_encode!(u32);
impl_int_encode!(u64);

impl_int_decode!(u8);
impl_int_decode!(u16);
impl_int_decode!(u32);
impl_int_decode!(u64);

impl Encode for Int<usize> {
    #[inline(always)]
    fn encode(&self, buf: &mut Vec<u8>) {
        Int(self.0 as u64).encode(buf)
    }
}

impl Decode for Int<usize> {
    type Value = usize;

    type Error = IntDecodeError;

    #[inline(always)]
    fn decode(buf: &[u8]) -> Result<(usize, &[u8]), Self::Error> {
        Int::<u64>::decode(buf).map(|(value, rest)| (value as usize, rest))
    }
}

macro_rules! impl_int_encode {
    ($ty:ty) => {
        impl Encode for Int<$ty> {
            #[inline]
            fn encode(&self, buf: &mut Vec<u8>) {
                let int = self.0;

                // We can encode the entire integer with a single byte if it
                // falls within this range.
                if int == 0 || (int > 8 && int <= u8::MAX as $ty) {
                    buf.push(int as u8);
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

                buf.push(len as u8);

                buf.extend_from_slice(&array[..len]);
            }
        }
    };
}

use impl_int_encode;

macro_rules! impl_int_decode {
    ($ty:ty) => {
        impl Decode for Int<$ty> {
            type Value = $ty;

            type Error = $crate::encode::IntDecodeError;

            #[inline]
            fn decode(buf: &[u8]) -> Result<($ty, &[u8]), Self::Error> {
                let (&len, buf) =
                    buf.split_first().ok_or(IntDecodeError::EmptyBuffer)?;

                if len == 0 || len > 8 {
                    let int = len as $ty;
                    return Ok((int, buf));
                }

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
        let ints = core::iter::once(0).chain(9..=u8::MAX as u64);

        let mut buf = Vec::new();

        for int in ints {
            Int::new(int).encode(&mut buf);
            assert_eq!(buf.len(), 1);
            let (decoded, rest) = Int::<u64>::decode(&buf).unwrap();
            assert_eq!(int, decoded);
            assert!(rest.is_empty());
            buf.clear();
        }
    }

    /// Tests that integers are encoded using the correct number of bytes.
    #[test]
    fn encode_int_num_bytes() {
        let ints = (1..=8).chain([
            u8::MAX as u64 + 1,
            u16::MAX as u64,
            u16::MAX as u64 + 1,
            u32::MAX as u64,
            u32::MAX as u64 + 1,
            u64::MAX,
        ]);

        let mut buf = Vec::new();

        // The highest number that can be represented with this many bytes.
        let max_num_with_n_bytes = |n_bytes: u8| {
            let bits = n_bytes * 8;
            // We use a u128 here to avoid overlowing if `n_bytes` is 8.
            ((1u128 << bits) - 1) as u64
        };

        for int in ints {
            Int::new(int).encode(&mut buf);

            let expected_len = (1..=8)
                .map(|n_bytes| (n_bytes, max_num_with_n_bytes(n_bytes)))
                .find_map(|(n_bytes, max_for_bytes)| {
                    (int <= max_for_bytes).then_some(n_bytes)
                })
                .unwrap();

            assert_eq!(buf[0], expected_len);

            assert_eq!(buf[1..].len() as u8, expected_len);

            let (decoded, rest) = Int::<u64>::decode(&buf).unwrap();

            assert_eq!(int, decoded);

            assert!(rest.is_empty());

            buf.clear();
        }
    }

    /// Tests that decoding an `Int` fails if the buffer is empty.
    #[test]
    fn encode_int_fails_if_buffer_empty() {
        let mut buf = Vec::new();

        Int::new(42u32).encode(&mut buf);

        buf.clear();

        assert_eq!(
            Int::<u32>::decode(&buf).unwrap_err(),
            IntDecodeError::EmptyBuffer
        );
    }

    /// Tests that decoding an `Int` fails if the length specified in the
    /// prefix is greater than the actual length of the buffer.
    #[test]
    fn encode_int_fails_if_buffer_too_short() {
        let mut buf = Vec::new();

        Int::new(u8::MAX as u16 + 1).encode(&mut buf);

        buf.pop();

        assert_eq!(
            Int::<u32>::decode(&buf).unwrap_err(),
            IntDecodeError::LengthLessThanPrefix { prefix: 2, actual: 1 }
        );
    }
}
