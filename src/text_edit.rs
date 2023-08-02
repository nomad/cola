use core::ops::Range;

use crate::*;

/// A range of text inserted into a [`Replica`].
///
/// Despite the name, this type does not contain the text string itself, only
/// the [`ReplicaId`] of the [`Replica`] that inserted it and its temporal
/// range in it. These can be accessed via the
/// [`inserted_by`](Text::inserted_by) and
/// [`temporal_range`](Text::temporal_range) methods respectively.
#[derive(Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "encode", derive(serde::Serialize, serde::Deserialize))]
pub struct Text {
    pub(crate) inserted_by: ReplicaId,
    pub(crate) range: Range<Length>,
}

impl core::fmt::Debug for Text {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:x}.{:?}", self.inserted_by, self.range)
    }
}

impl Text {
    #[inline]
    pub(crate) fn end(&self) -> Length {
        self.range.end
    }

    /// Returns the [`ReplicaId`] of the [`Replica`] that inserted this text.
    ///
    /// # Examples
    /// ```
    /// # use cola::Replica;
    /// let mut replica1 = Replica::new(1, 0);
    ///
    /// let insertion = replica1.inserted(0, 1);
    ///
    /// assert_eq!(insertion.text().inserted_by(), replica1.id());
    /// ```
    #[inline]
    pub fn inserted_by(&self) -> ReplicaId {
        self.inserted_by
    }

    #[inline]
    pub(crate) fn len(&self) -> Length {
        self.range.len()
    }

    #[inline]
    pub(crate) fn new(inserted_by: ReplicaId, range: Range<Length>) -> Self {
        Self { inserted_by, range }
    }

    #[inline]
    pub(crate) fn start(&self) -> Length {
        self.range.start
    }

    /// Returns the temporal range of this text in the `Replica` that inserted
    /// it.
    ///
    /// Each `Replica` keeps an internal character clock that starts at zero
    /// and is incremented each time the [`inserted`](crate::Replica::inserted)
    /// method is called by an amount equal to the length of the inserted text.
    ///
    /// Since the insertion history of *a single* `Replica` is linear and
    /// immutable, this clock can be used to uniquely identify each character
    /// in the document without knowing what the actual text associated with
    /// its insertion is.
    ///
    /// Note that this range has absolutely nothing to do with the *spatial
    /// offset* at which the text was inserted. Its start and end simply refer
    /// to the values of the character clock of the `Replica` that inserted the
    /// `Text` before and after the insertion.
    ///
    /// It's up to you to decide how to map these temporal ranges to the actual
    /// text contents inserted by the various peers.
    ///
    /// # Examples
    /// ```
    /// # use cola::Replica;
    /// let mut replica1 = Replica::new(1, 0);
    ///
    /// // Peer 1 inserts 1, 2, 3 and 4 characters at the start of the
    /// // document.
    /// let insertion1 = replica1.inserted(0, 1);
    /// let insertion2 = replica1.inserted(0, 2);
    /// let insertion3 = replica1.inserted(0, 3);
    /// let insertion4 = replica1.inserted(0, 4);
    ///
    /// // Notice how:
    /// // - the temporal range of the first insertion starts at zero;
    /// // - the start of each range is equal to the end of the previous one;
    /// // - the length of each range matches the one passed to
    /// //   `replica1.inserted`.
    ///
    /// assert_eq!(insertion1.text().temporal_range(), 0..1);
    /// assert_eq!(insertion2.text().temporal_range(), 1..3);
    /// assert_eq!(insertion3.text().temporal_range(), 3..6);
    /// assert_eq!(insertion4.text().temporal_range(), 6..10);
    /// ```
    #[inline]
    pub fn temporal_range(&self) -> Range<Length> {
        self.range.clone()
    }
}
