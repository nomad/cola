use core::ops::Range;

use crate::{Length, Text};

/// A plain text edit to be applied to a buffer.
///
/// This enum is either created by the [`merge`](crate::Replica::merge) method
/// on [`Replica`](crate::Replica) or yielded by the
/// [`BackLogged`](crate::BackLogged) iterator. See their documentation for
/// more.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TextEdit {
    /// This variant represents the offset at which to perform an insertion and
    /// the corresponding `Text`.
    ///
    /// Note that this variant doesn't actually store the text string to be
    /// inserted into your buffer (nor does a [`Replica`](crate::Replica) ask
    /// for it when calling [`inserted`](crate::Replica::inserted)).
    ///
    /// You, as the user of the library, are responsible for sending the
    /// inserted contents from the peer that performed the insertion to the
    /// peer that received this enum using the transport layer of your choice.
    Insertion(Length, Text),

    /// This variant represents an offset range to be deleted.
    ///
    /// This variant is returned when there haven't been any concurrent edits
    /// in the original region of text from the time the deletion was performed
    /// to when it was merged into the replica that created this [`TextEdit`].
    ///
    /// In this case the deleted region is still contiguous and can be
    /// represented with a single range.
    ///
    /// The start of the range is guaranteed to be strictly less than the end.
    ///
    /// # Example
    ///
    /// ```
    /// # use cola::{Replica, TextEdit};
    /// // Peer 1 starts with the text "abcd", and sends it to a second peer.
    /// let mut replica1 = Replica::new(4);
    ///
    /// let mut replica2 = replica1.clone();
    ///
    /// // Peer 1 deletes the "bc" in "abcd".
    /// let deletion = replica1.delete(1..3);
    ///
    /// // Concurrently, peer 2 inserts a single character at start of the
    /// // document.
    /// let _ = replica2.inserted(0, 1);
    ///
    /// // Now peer 2 receives the deletion from peer 1. Since the previous
    /// // insertion was outside of the deleted region the latter is still
    /// // contiguous at this peer.
    /// let Some(TextEdit::ContiguousDeletion(range)) = replica2.merge(&deletion)
    /// else {
    ///     unreachable!();
    /// };
    ///
    /// assert_eq!(range, 2..4);
    /// ```
    ContiguousDeletion(Range<Length>),

    /// This variant represents a series of offset ranges to be deleted.
    ///
    /// This variant is returned when there's been one or more concurrent edits
    /// in the original region of text from the time the deletion was performed
    /// to when it was merged into the replica that created this [`TextEdit`].
    ///
    /// In this case the deleted region has been split into multiple ranges
    /// which are stored in a [`Vec`].
    ///
    /// The ranges in the vector are guaranteed to be sorted in ascending order
    /// and to not overlap, i.e. for any two indices `i` and `j` where `i < j`
    /// and `j < ranges.len()` it holds that `ranges[i].end < ranges[j].start`
    /// (and of course that `ranges[i].start < ranges[i].end`).
    ///
    /// # Example
    ///
    /// ```
    /// # use cola::{Replica, TextEdit};
    /// // Peer 1 starts with the text "abcd", and sends it to a second peer.
    /// let mut replica1 = Replica::new(4);
    ///
    /// let mut replica2 = replica1.clone();
    ///
    /// // Peer 1 deletes the "bc" in "abcd".
    /// let deletion = replica1.delete(1..3);
    ///
    /// // Concurrently, peer 2 inserts a single character between the 'b' and
    /// // the 'c'.
    /// let _ = replica2.inserted(2, 1);
    ///
    /// // Now peer 2 receives the deletion from peer 1. Since the previous
    /// // insertion was inside the deleted range, the latter has now been
    /// // split into two separate ranges.
    /// let Some(TextEdit::SplitDeletion(ranges)) = replica2.merge(&deletion)
    /// else {
    ///     unreachable!();
    /// };
    ///
    /// assert_eq!(ranges.as_slice(), &[1..2, 3..4]);
    /// ```
    SplitDeletion(Vec<Range<Length>>),
}
