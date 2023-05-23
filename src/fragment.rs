use core::ops::{Add, AddAssign, Range};

use super::{EditId, LamportTimestamp};
use crate::node::Summarize;

/// TODO: docs
#[derive(Copy, Clone, Default)]
pub struct Fragment {
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
    pub fn delete(&mut self) {
        self.is_visible = false;
    }

    #[inline]
    pub fn delete_from(&mut self, offset: usize) -> Option<Self> {
        self.split(offset).map(|mut del| {
            del.is_visible = false;
            del
        })
    }

    #[inline]
    pub fn delete_range(
        &mut self,
        Range { start, end }: Range<usize>,
    ) -> (Option<Self>, Option<Self>) {
        debug_assert!(start <= end);

        let rest = self.split(end);

        let deleted = self.split(start).map(|mut del| {
            del.is_visible = false;
            del
        });

        (deleted, rest)
    }

    #[inline]
    pub fn delete_up_to(&mut self, offset: usize) -> Option<Self> {
        let rest = self.split(offset);
        self.is_visible = false;
        rest
    }

    #[inline]
    pub(super) fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub(super) fn id(&self) -> EditId {
        self.id
    }

    #[inline]
    pub(crate) fn new(
        id: EditId,
        parent: EditId,
        offset_in_parent: usize,
        timestamp: LamportTimestamp,
        len: usize,
    ) -> Self {
        Self { id, parent, offset_in_parent, timestamp, len, is_visible: true }
    }

    /// TODO: docs
    #[inline]
    pub fn split(&mut self, byte_offset: usize) -> Option<Self> {
        if byte_offset < self.len {
            let mut rest = *self;
            rest.len = self.len - byte_offset;
            self.len = byte_offset;
            Some(rest)
        } else {
            None
        }
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
