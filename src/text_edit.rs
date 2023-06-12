use alloc::borrow::Cow;
use core::ops::Range;

use crate::Length;

/// TODO: docs
pub struct TextEdit<'a> {
    /// TODO: docs
    pub range: Range<Length>,

    /// TODO: docs
    pub content: Cow<'a, str>,
}

impl<'a> TextEdit<'a> {
    #[inline]
    pub(crate) fn new(content: Cow<'a, str>, range: Range<Length>) -> Self {
        Self { content, range }
    }
}
