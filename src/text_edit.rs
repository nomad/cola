use core::ops::Range;

/// TODO: docs
pub struct TextEdit {
    /// TODO: docs
    pub range: Range<usize>,
}

impl TextEdit {
    #[inline]
    pub(crate) fn _new(range: Range<usize>) -> Self {
        Self { range }
    }
}
