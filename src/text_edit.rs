use core::ops::Range;

use crate::Length;

/// TODO: docs
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TextEdit {
    /// TODO: docs
    Insertion(Length),

    /// TODO: docs
    ContiguousDeletion(Range<Length>),

    /// TODO: docs
    SplitDeletion(Vec<Range<Length>>),
}
