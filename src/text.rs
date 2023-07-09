use core::ops::Range;

use crate::{Length, ReplicaId};

/// A range of text inserted into a `Replica`.
///
/// Despite the name, this type does not contain the text string itself, only
/// the id of the `Replica` that inserted it and its *temporal* range in the
/// `Replica`. These two properties can be accessed via the
/// [`inserted_by`](Text::inserted_by) and
/// [`temporal_range`](Text::temporal_range) methods respectively.
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct Text {
    pub(crate) inserted_by: ReplicaId,
    pub(crate) range: crate::Range<Length>,
}

impl core::fmt::Debug for Text {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:x}.{:?}", self.inserted_by.as_u32(), self.range)
    }
}

impl Text {
    /// Returns the id of the `Replica` that inserted this text.
    ///
    /// # Examples
    /// ```
    /// # use cola::{Replica, TextEdit};
    /// // Two peers start collaborating on an empty document.
    /// let mut replica1 = Replica::new(0);
    /// let mut replica2 = replica1.clone();
    ///
    /// // Peer 1 inserts a character.
    /// let insertion = replica1.inserted(0, 1);
    ///
    /// // Peer 2 merges the insertion.
    /// let Some(TextEdit::Insertion(_, text)) = replica2.merge(&insertion) else {
    ///     unreachable!();
    /// };
    ///
    /// // The text was inserted by peer 1.
    /// assert_eq!(text.inserted_by(), replica1.id());
    /// ```
    #[inline]
    pub fn inserted_by(&self) -> ReplicaId {
        self.inserted_by
    }

    #[inline]
    pub(crate) fn new(
        inserted_by: ReplicaId,
        range: crate::Range<Length>,
    ) -> Self {
        Self { inserted_by, range }
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
    /// # use cola::{Replica};
    /// fn unwrap_text(edit: Option<TextEdit>) -> Text {
    ///     if let Some(TextEdit::Insertion(_, text)) = edit {
    ///         text
    ///     } else {
    ///         unreachable!();
    ///     }
    /// }
    ///
    /// // Two peers start collaborating on an empty document.
    /// let mut replica1 = Replica::new(0);
    /// let mut replica2 = replica1.clone();
    ///
    /// // Peer 1 inserts 1, 2, 3 and 4 characters at the start of the
    /// // document.
    /// let insertion1 = replica1.inserted(0, 1);
    /// let insertion2 = replica1.inserted(0, 2);
    /// let insertion3 = replica1.inserted(0, 3);
    /// let insertion4 = replica1.inserted(0, 4);
    ///
    /// // Peer 2 merges the insertions.
    /// //
    /// // Notice how:
    /// // - the temporal range of the first insertion starts at zero;
    /// // - the start of each subsequent range is equal to the end of the
    /// //   previous one;
    /// // - the length of each range is equal to the length given to
    /// //   `replica1.insert`.
    ///
    /// let text1 = unwrap_text(replica2.merge(&insertion1));
    /// assert_eq!(text1.temporal_range(), 0..1);
    ///
    /// let text2 = unwrap_text(replica2.merge(&insertion2));
    /// assert_eq!(text2.temporal_range(), 1..3);
    ///
    /// let text3 = unwrap_text(replica2.merge(&insertion3));
    /// assert_eq!(text3.temporal_range(), 3..6);
    ///
    /// let text4 = unwrap_text(replica2.merge(&insertion4));
    /// assert_eq!(text4.temporal_range(), 6..10);
    /// ```
    #[inline]
    pub fn temporal_range(&self) -> Range<Length> {
        self.range.into()
    }
}
