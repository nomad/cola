use core::ops::Range;

use crate::{Length, ReplicaId};

/// A plain text edit to be applied to a buffer.
///
/// This enum is either created by the [`merge`](crate::Replica::merge) method
/// on [`Replica`](crate::Replica) or yielded by the
/// [`Backlogged`](crate::Backlogged) iterator. See their documentation for
/// more.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(dead_code)]
pub enum TextEdit {
    // This variant represents the offset at which to perform an insertion and
    // the corresponding `Text`.
    //
    // Note that this variant doesn't actually store the text string to be
    // inserted into your buffer (nor does a [`Replica`](crate::Replica) ask
    // for it when calling [`inserted`](crate::Replica::inserted)).
    //
    // You, as the user of the library, are responsible for sending that
    // string from the peer that performed the insertion to the peer that
    // created this enum using the transport layer of your choice.
    Insertion(Length, Text),

    // This variant represents an offset range to be deleted.
    //
    // This variant is returned when there haven't been any concurrent edits
    // in the original region of text from the time the deletion was performed
    // to when it was merged into the replica that created this [`TextEdit`].
    //
    // In this case the deleted region is still contiguous and can be
    // represented with a single range.
    //
    // The start of the range is guaranteed to be strictly less than the end.
    //
    // # Example
    //
    // ```
    // # use cola::{Replica, TextEdit};
    // // Peer 1 starts with the text "abcd", and sends it to a second peer.
    // let mut replica1 = Replica::new(1, 4);
    //
    // let mut replica2 = replica1.fork(2);
    //
    // // Peer 1 deletes the "bc" in "abcd".
    // let deletion = replica1.deleted(1..3);
    //
    // // Concurrently, peer 2 inserts a single character at start of the
    // // document.
    // let _ = replica2.inserted(0, 1);
    //
    // // Now peer 2 receives the deletion from peer 1. Since the previous
    // // insertion was outside of the deleted region the latter is still
    // // contiguous at this peer.
    // let Some(TextEdit::Deletion(range)) = replica2.merge(&deletion) else {
    //     unreachable!();
    // };
    //
    // assert_eq!(range.as_slice(), &[2..4]);
    // ```
    ContiguousDeletion(Range<Length>),

    // This variant represents a series of offset ranges to be deleted.
    //
    // This variant is returned when there's been one or more concurrent edits
    // in the original region of text from the time the deletion was performed
    // to when it was merged into the replica that created this [`TextEdit`].
    //
    // In this case the deleted region has been split into multiple ranges
    // which are stored in a [`Vec`].
    //
    // The ranges in the vector are guaranteed to be sorted in ascending order
    // and to not overlap, i.e. for any two indices `i` and `j` where `i < j`
    // and `j < ranges.len()` it holds that `ranges[i].end < ranges[j].start`
    // (and of course that `ranges[i].start < ranges[i].end`).
    //
    // # Example
    //
    // ```
    // # use cola::{Replica, TextEdit};
    // // Peer 1 starts with the text "abcd", and sends it to a second peer.
    // let mut replica1 = Replica::new(1, 4);
    //
    // let mut replica2 = replica1.fork(2);
    //
    // // Peer 1 deletes the "bc" in "abcd".
    // let deletion = replica1.deleted(1..3);
    //
    // // Concurrently, peer 2 inserts a single character between the 'b' and
    // // the 'c'.
    // let _ = replica2.inserted(2, 1);
    //
    // // Now peer 2 receives the deletion from peer 1. Since the previous
    // // insertion was inside the deleted range, the latter has now been
    // // split into two separate ranges.
    // let Some(TextEdit::Deletion(ranges)) = replica2.merge(&deletion) else {
    //     unreachable!();
    // };
    //
    // assert_eq!(ranges.as_slice(), &[1..2, 3..4]);
    // ```
    Deletion(Vec<Range<Length>>),
}

/// A range of text inserted into a `Replica`.
///
/// Despite the name, this type does not contain the text string itself, only
/// the id of the `Replica` that inserted it and its *temporal* range in the
/// `Replica`. These two properties can be accessed via the
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
        write!(f, "{:x}.{:?}", self.inserted_by.as_u32(), self.range)
    }
}

impl Text {
    #[inline]
    pub(crate) fn end(&self) -> Length {
        self.range.end
    }

    /// Returns the id of the `Replica` that inserted this text.
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
    /// Each `Replica` has an internal character clock that starts at zero and
    /// is incremented each time the [`inserted`](crate::Replica::inserted)
    /// method is called. The amount by which the clock is incremented is equal
    /// to the length of the inserted text.
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
    /// // Two peers start collaborating on an empty document.
    /// let mut replica1 = Replica::new(1, 0);
    /// let mut replica2 = replica1.fork(2);
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
    /// // - the start of each subsequent range is equal to the end of the
    /// //   previous one;
    /// // - the length of each range is equal to the length given to
    /// //   `replica1.insert`.
    ///
    /// assert_eq!(insertion1.text().temporal_range(), 0..1);
    ///
    /// assert_eq!(insertion2.text().temporal_range(), 1..3);
    ///
    /// assert_eq!(insertion3.text().temporal_range(), 3..6);
    ///
    /// assert_eq!(insertion4.text().temporal_range(), 6..10);
    /// ```
    #[inline]
    pub fn temporal_range(&self) -> Range<Length> {
        self.range.clone()
    }
}
