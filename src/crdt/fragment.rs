use core::ops::{Add, AddAssign};

use super::{EditId, LamportTimestamp};
use crate::tree::Summarize;

/// TODO: docs
#[derive(Copy, Clone, Default)]
pub(super) struct Fragment {
    /// TODO: docs
    id: EditId,

    /// TODO: docs
    timestamp: LamportTimestamp,

    /// TODO: docs
    parent: EditId,

    /// TODO: docs
    offset_in_parent: usize,

    /// TODO: docs
    len: usize,

    /// TODO: docs
    is_visible: bool,
}

impl core::fmt::Debug for Fragment {
    #[inline]
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(
            f,
            "{:?} L({}) |> {:?} @ {}",
            self.id,
            self.timestamp.as_u64(),
            self.parent,
            self.offset_in_parent
        )
    }
}

impl Fragment {
    #[inline]
    pub(super) fn id(&self) -> EditId {
        self.id
    }

    #[inline]
    pub(super) fn new(
        id: EditId,
        parent: EditId,
        offset_in_parent: usize,
        timestamp: LamportTimestamp,
        len: usize,
    ) -> Self {
        Self { id, parent, offset_in_parent, timestamp, len, is_visible: true }
    }
}

/// TODO: docs
#[derive(Clone, Copy, Default, PartialEq)]
pub struct FragmentSummary {
    pub(super) len: usize,
    pub(super) is_visible: bool,
}

impl core::fmt::Debug for FragmentSummary {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "{{ len: {}, is_visible: {} }}", self.len, self.is_visible)
    }
}

impl Add<Self> for FragmentSummary {
    type Output = Self;

    #[inline]
    fn add(mut self, rhs: Self) -> Self {
        self += rhs;
        self
    }
}

impl AddAssign<Self> for FragmentSummary {
    #[inline]
    fn add_assign(&mut self, rhs: Self) {
        self.len = (self.is_visible as usize) * self.len
            + (rhs.is_visible as usize) * rhs.len;

        self.is_visible |= rhs.is_visible;
    }
}

impl Summarize for Fragment {
    type Summary = FragmentSummary;

    #[inline]
    fn summarize(&self) -> Self::Summary {
        FragmentSummary { len: self.len, is_visible: self.is_visible }
    }
}
