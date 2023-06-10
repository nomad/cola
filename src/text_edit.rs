use alloc::borrow::Cow;
use core::ops::Range;

/// TODO: docs
pub struct TextEdit<'a> {
    /// TODO: docs
    pub range: Range<usize>,

    /// TODO: docs
    pub content: Cow<'a, str>,
}

impl<'a> TextEdit<'a> {
    #[inline]
    pub(crate) fn new(content: Cow<'a, str>, range: Range<usize>) -> Self {
        Self { content, range }
    }
}
