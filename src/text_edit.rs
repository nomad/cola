use core::ops::Range;

/// TODO: docs
pub struct TextEdit {
    pub range: Range<usize>,
    pub contents: String,
}
