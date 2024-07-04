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
                let (array, len) = varint_simd::encode(*self);
                buf.extend_from_slice(&array[..len as usize]);
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
                let (decoded, len) = varint_simd::decode::<Self>(buf)
                    .map_err(IntDecodeError)?;

                // TODO: this check shouldn't be necessary, `decode` should
                // fail. Open an issue.
                let Some(rest) = buf.get(len as usize..) else {
                    return Err(IntDecodeError(
                        varint_simd::VarIntDecodeError::NotEnoughBytes,
                    ));
                };

                Ok((decoded, rest))
            }
        }
    };
}

use impl_int_decode;

/// An error that can occur when decoding an [`Int`].
pub(crate) struct IntDecodeError(varint_simd::VarIntDecodeError);

impl Display for IntDecodeError {
    #[inline]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        Display::fmt(&self.0, f)
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

    impl PartialEq for IntDecodeError {
        fn eq(&self, other: &Self) -> bool {
            use varint_simd::VarIntDecodeError::*;
            matches!(
                (&self.0, &other.0),
                (Overflow, Overflow) | (NotEnoughBytes, NotEnoughBytes)
            )
        }
    }

    impl core::fmt::Debug for IntDecodeError {
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

    /// Tests the encoding-decoding roundtrip on a number of inputs.
    #[test]
    fn encode_int_roundtrip() {
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
            IntDecodeError(varint_simd::VarIntDecodeError::NotEnoughBytes),
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
            IntDecodeError(varint_simd::VarIntDecodeError::NotEnoughBytes),
        );
    }
}
