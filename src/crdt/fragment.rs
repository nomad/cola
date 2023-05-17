use core::ops::{Add, AddAssign};

use super::{LamportTimestamp, LocalTimestamp, ReplicaId};
use crate::tree::Summarize;

/// TODO: docs
#[derive(Debug, Clone)]
pub(super) struct Fragment {
    /// TODO: docs
    edit: EditId,

    /// TODO: docs
    parent: EditId,

    /// TODO: docs
    offset_in_parent: usize,

    /// TODO: docs
    lamport_timestamp: LamportTimestamp,

    /// TODO: docs
    len: usize,

    /// TODO: docs
    is_visible: bool,
}

/// TODO: docs
#[derive(Clone, Copy, Debug)]
pub(super) struct EditId {
    /// TODO: docs
    created_by: ReplicaId,

    /// TODO: docs
    local_timestamp_at_creation: LocalTimestamp,
}

/// TODO: docs
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FragmentSummary {
    len: usize,
    is_visible: bool,
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
