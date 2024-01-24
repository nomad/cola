use crate::anchor::InnerAnchor as Anchor;
use crate::{LamportTs, Length, ReplicaId, RunTs, Text};

/// An insertion in CRDT coordinates.
///
/// This struct is created by the [`inserted`](Replica::inserted) method on the
/// [`Replica`] owned by the peer that performed the insertion, and can be
/// integrated by another [`Replica`] via the
/// [`integrate_insertion`](Replica::integrate_insertion) method.
///
/// See the documentation of those methods for more information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Insertion {
    /// The anchor point of the insertion.
    anchor: Anchor,

    /// Contains the replica that made the insertion and the temporal range
    /// of the text that was inserted.
    text: Text,

    /// The run timestamp of this insertion.
    run_ts: RunTs,

    /// The Lamport timestamp of this insertion.
    lamport_ts: LamportTs,
}

impl Insertion {
    #[inline(always)]
    pub(crate) fn anchor(&self) -> Anchor {
        self.anchor
    }

    #[inline(always)]
    pub(crate) fn end(&self) -> Length {
        self.text.range.end
    }

    #[inline(always)]
    pub(crate) fn inserted_by(&self) -> ReplicaId {
        self.text.inserted_by()
    }

    #[inline]
    pub(crate) fn is_no_op(&self) -> bool {
        self.len() == 0
    }

    #[inline(always)]
    pub(crate) fn run_ts(&self) -> RunTs {
        self.run_ts
    }

    #[inline(always)]
    pub(crate) fn lamport_ts(&self) -> LamportTs {
        self.lamport_ts
    }

    #[inline]
    pub(crate) fn len(&self) -> Length {
        self.text.len()
    }

    #[inline]
    pub(crate) fn new(
        anchor: Anchor,
        text: Text,
        lamport_ts: LamportTs,
        run_ts: RunTs,
    ) -> Self {
        Self { anchor, text, lamport_ts, run_ts }
    }

    #[inline]
    pub(crate) fn no_op() -> Self {
        Self::new(Anchor::zero(), Text::new(0, 0..0), 0, 0)
    }

    #[inline]
    pub(crate) fn start(&self) -> Length {
        self.text.range.start
    }

    /// The [`Text`] of this insertion.
    #[inline]
    pub fn text(&self) -> &Text {
        &self.text
    }
}

#[cfg(feature = "encode")]
mod encode {
    use super::*;
    use crate::encode::{Decode, Encode, Int, IntDecodeError};

    impl Insertion {
        #[inline]
        fn encode_anchor(&self, run: InsertionRun, buf: &mut Vec<u8>) {
            match run {
                InsertionRun::BeginsNew => self.anchor.encode(buf),
                InsertionRun::ContinuesExisting => {},
            }
        }

        #[inline]
        fn decode_anchor<'buf>(
            run: InsertionRun,
            text: &Text,
            run_ts: RunTs,
            buf: &'buf [u8],
        ) -> Result<(Anchor, &'buf [u8]), <Anchor as Decode>::Error> {
            match run {
                InsertionRun::BeginsNew => Anchor::decode(buf),

                InsertionRun::ContinuesExisting => {
                    let anchor =
                        Anchor::new(text.inserted_by(), text.start(), run_ts);
                    Ok((anchor, buf))
                },
            }
        }
    }

    impl Encode for Insertion {
        #[inline]
        fn encode(&self, buf: &mut Vec<u8>) {
            self.text.encode(buf);
            Int::new(self.run_ts).encode(buf);
            Int::new(self.lamport_ts).encode(buf);
            let run = InsertionRun::new(self);
            run.encode(buf);
            self.encode_anchor(run, buf);
        }
    }

    impl Decode for Insertion {
        type Value = Self;

        type Error = IntDecodeError;

        #[inline]
        fn decode(buf: &[u8]) -> Result<(Self, &[u8]), Self::Error> {
            let (text, buf) = Text::decode(buf)?;
            let (run_ts, buf) = Int::<RunTs>::decode(buf)?;
            let (lamport_ts, buf) = Int::<LamportTs>::decode(buf)?;
            let (run, buf) = InsertionRun::decode(buf).unwrap();
            let (anchor, buf) = Self::decode_anchor(run, &text, run_ts, buf)?;
            let insertion = Self::new(anchor, text, run_ts, lamport_ts);
            Ok((insertion, buf))
        }
    }

    /// TODO: docs
    enum InsertionRun {
        /// TODO: docs
        BeginsNew,

        /// TODO: docs
        ContinuesExisting,
    }

    impl InsertionRun {
        #[inline]
        fn new(insertion: &Insertion) -> Self {
            let is_continuation = insertion.anchor.replica_id()
                == insertion.text.inserted_by()
                && insertion.anchor.offset() == insertion.text.start();

            if is_continuation {
                Self::ContinuesExisting
            } else {
                Self::BeginsNew
            }
        }
    }

    impl Encode for InsertionRun {
        #[inline]
        fn encode(&self, buf: &mut Vec<u8>) {
            let is_continuation = matches!(self, Self::ContinuesExisting);
            buf.push(is_continuation as u8);
        }
    }

    impl Decode for InsertionRun {
        type Value = Self;

        type Error = core::convert::Infallible;

        #[inline]
        fn decode(buf: &[u8]) -> Result<(Self, &[u8]), Self::Error> {
            let Some((&first_byte, rest)) = buf.split_first() else { todo!() };

            let this = match first_byte {
                0 => Self::BeginsNew,
                1 => Self::ContinuesExisting,
                _other => todo!(),
            };

            Ok((this, rest))
        }
    }
}

#[cfg(feature = "serde")]
mod serde {
    crate::impl_deserialize!(super::Insertion);
    crate::impl_serialize!(super::Insertion);
}
