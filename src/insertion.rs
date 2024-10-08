use crate::anchor::InnerAnchor as Anchor;
use crate::{LamportTs, Length, ReplicaId, RunTs, Text};

/// An insertion in CRDT coordinates.
///
/// This struct is created by the [`inserted`] method on the [`Replica`] owned
/// by the peer that performed the insertion, and can be integrated by another
/// [`Replica`] via the [`integrate_insertion`] method.
///
/// See the documentation of those methods for more information.
///
/// [`Replica`]: crate::Replica
/// [`inserted`]: crate::Replica::inserted
/// [`integrate_insertion`]: crate::Replica::integrate_insertion
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

    /// Returns the [`ReplicaId`] of the [`Replica`](crate::Replica) that
    /// performed the insertion.
    ///
    /// # Examples
    ///
    /// ```
    /// # use cola::Replica;
    /// let mut replica = Replica::new(1, 3);
    /// let insertion = replica.inserted(3, 7);
    /// assert_eq!(insertion.inserted_by(), replica.id());
    /// ```
    #[inline]
    pub fn inserted_by(&self) -> ReplicaId {
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
    use crate::encode::{BoolDecodeError, Decode, Encode, IntDecodeError};

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
            self.run_ts.encode(buf);
            self.lamport_ts.encode(buf);
            let run = InsertionRun::new(self);
            run.encode(buf);
            self.encode_anchor(run, buf);
        }
    }

    pub(crate) enum InsertionDecodeError {
        Int(IntDecodeError),
        Run(BoolDecodeError),
    }

    impl From<IntDecodeError> for InsertionDecodeError {
        #[inline]
        fn from(err: IntDecodeError) -> Self {
            Self::Int(err)
        }
    }

    impl From<BoolDecodeError> for InsertionDecodeError {
        #[inline]
        fn from(err: BoolDecodeError) -> Self {
            Self::Run(err)
        }
    }

    impl core::fmt::Display for InsertionDecodeError {
        #[inline]
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            let err: &dyn core::fmt::Display = match self {
                Self::Int(err) => err,
                Self::Run(err) => err,
            };

            write!(f, "InsertionRun couldn't be decoded: {err}")
        }
    }

    impl Decode for Insertion {
        type Value = Self;

        type Error = InsertionDecodeError;

        #[inline]
        fn decode(buf: &[u8]) -> Result<(Self, &[u8]), Self::Error> {
            let (text, buf) = Text::decode(buf)?;
            let (run_ts, buf) = RunTs::decode(buf)?;
            let (lamport_ts, buf) = LamportTs::decode(buf)?;
            let (run, buf) = InsertionRun::decode(buf)?;
            let (anchor, buf) = Self::decode_anchor(run, &text, run_ts, buf)?;
            let insertion = Self::new(anchor, text, lamport_ts, run_ts);
            Ok((insertion, buf))
        }
    }

    /// Whether an [`Insertion`] begins a new run or continues an existing one.
    ///
    /// This is used when encoding and decoding [`Insertion`]s to determine
    /// whether their [`Anchor`] needs to be encoded.
    ///
    /// Most of the time when people edit a document they insert a bunch of
    /// characters in a single run before moving the cursor or deleting some
    /// text, and we can use this pattern to save some bytes.
    ///
    /// For example, if someone types "foo" sequentially in a blank document,
    /// we'll create the following insertions (assuming a `ReplicaId` of 1 and
    /// omitting fields that aren't relevant to this discussion):
    ///
    /// ```text
    /// f -> Insertion { anchor: zero, text: 1.0..1, .. },
    /// o -> Insertion { anchor: 1.1, text: 1.1..2, .. },
    /// o -> Insertion { anchor: 1.2, text: 1.2..3, .. },
    /// ```
    ///
    /// The first insertion begins a new run, but from then on every
    /// Insertion's anchor is the same as the start of its text.
    ///
    /// This means that we can save space when encoding by omitting the anchor
    /// and adding a flag that indicates that it should be derived from the
    /// text and the run timestamp.
    ///
    /// This enum corresponds to that flag.
    enum InsertionRun {
        /// The [`Insertion`] begins a new run.
        ///
        /// In this case we also encode the insertion's [`Anchor`].
        BeginsNew,

        /// The [`Insertion`] continues an existing run.
        ///
        /// In this case we can avoid encoding the insertion's [`Anchor`]
        /// because it can be fully decoded from the insertion's [`Text`] and
        /// [`RunTs`].
        ContinuesExisting,
    }

    impl InsertionRun {
        #[inline]
        fn new(insertion: &Insertion) -> Self {
            // To determine whether this insertion is a continuation of an
            // existing insertion run we simply check:
            //
            // 1: the `ReplicaId`s of the anchor and the text. Clearly they
            //    must match because you can't continue someone else's
            //    insertion;
            //
            // 2: the `RunTs` of the anchor and the insertion. Since that
            //    counter is only incremented when a new insertion run begins,
            //    we know that if they match then this insertion must continue
            //    an existing run.
            let is_continuation = insertion.anchor.replica_id()
                == insertion.text.inserted_by()
                && insertion.anchor.run_ts() == insertion.run_ts();

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
            matches!(self, Self::ContinuesExisting).encode(buf);
        }
    }

    impl Decode for InsertionRun {
        type Value = Self;

        type Error = BoolDecodeError;

        #[inline]
        fn decode(buf: &[u8]) -> Result<(Self, &[u8]), Self::Error> {
            let (is_continuation, rest) = bool::decode(buf)?;
            let this = if is_continuation {
                Self::ContinuesExisting
            } else {
                Self::BeginsNew
            };
            Ok((this, rest))
        }
    }
}

#[cfg(feature = "serde")]
mod serde {
    crate::encode::impl_deserialize!(super::Insertion);
    crate::encode::impl_serialize!(super::Insertion);
}

#[cfg(all(test, feature = "encode"))]
mod encode_tests {
    use super::*;
    use crate::encode::{Decode, Encode};

    impl core::fmt::Debug for encode::InsertionDecodeError {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            core::fmt::Display::fmt(self, f)
        }
    }

    #[test]
    fn encode_insertion_round_trip_0() {
        let anchor = Anchor::new(1, 1, 1);
        let text = Text::new(2, 0..1);
        let insertion = Insertion::new(anchor, text, 3, 0);
        let mut buf = Vec::new();
        insertion.encode(&mut buf);
        let (decoded, rest) = Insertion::decode(&buf).unwrap();
        assert_eq!(insertion, decoded);
        assert!(rest.is_empty());
    }
}
