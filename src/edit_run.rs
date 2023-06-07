use core::ops::{Add, AddAssign, Range, Sub, SubAssign};

use super::{EditId, LamportTimestamp};
use crate::node::Summarize;

/// TODO: docs
#[derive(Clone, Default)]
pub struct EditRun {
    /// TODO: docs
    edit_id: EditId,

    /// TODO: docs
    run_id: RunId,

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

impl core::fmt::Debug for EditRun {
    #[inline]
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(
            f,
            "{:?} L({}) |> {:?} @ {}, {} {}",
            self.edit_id,
            self.timestamp.as_u64(),
            self.parent,
            self.offset_in_parent,
            self.len,
            if self.is_visible { "✔️" } else { "🪦" },
        )
    }
}

impl EditRun {
    #[inline]
    pub fn delete(&mut self) {
        self.is_visible = false;
    }

    #[inline]
    pub fn delete_from(&mut self, offset: usize) -> Option<Self> {
        if offset == 0 {
            self.is_visible = false;
            None
        } else {
            self.split(offset).map(|mut del| {
                del.is_visible = false;
                del
            })
        }
    }

    #[inline]
    pub fn delete_range(
        &mut self,
        Range { start, end }: Range<usize>,
    ) -> (Option<Self>, Option<Self>) {
        debug_assert!(start <= end);

        if start == 0 {
            (self.delete_up_to(end), None)
        } else if end >= self.len {
            (self.delete_from(start), None)
        } else {
            let rest = self.split(end);

            let deleted = self.split(start).map(|mut del| {
                del.is_visible = false;
                del
            });

            (deleted, rest)
        }
    }

    #[inline]
    pub fn delete_up_to(&mut self, offset: usize) -> Option<Self> {
        if offset == 0 {
            None
        } else {
            let rest = self.split(offset);
            self.is_visible = false;
            rest
        }
    }

    #[inline]
    pub(super) fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub(super) fn id(&self) -> EditId {
        self.edit_id
    }

    #[inline]
    pub(crate) fn new(
        edit_id: EditId,
        run_id: RunId,
        parent: EditId,
        offset_in_parent: usize,
        timestamp: LamportTimestamp,
        len: usize,
    ) -> Self {
        Self {
            edit_id,
            run_id,
            parent,
            offset_in_parent,
            timestamp,
            len,
            is_visible: true,
        }
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
///
/// The `Ord` implementation for `Vec`s is a lexicographic sort, so we can
/// derive those traits.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct RunId {
    /// TODO: docs
    letters: Vec<u16>,
}

impl AddAssign<&Self> for RunId {
    #[inline]
    fn add_assign(&mut self, other: &Self) {
        if &*self < other {
            *self = other.clone();
        }
    }
}

impl SubAssign<&Self> for RunId {
    #[inline]
    fn sub_assign(&mut self, other: &Self) {
        if &*self > other {
            *self = other.clone();
        }
    }
}

/// TODO: docs
#[derive(Clone, Default, PartialEq)]
pub struct RunSummary {
    /// TODO: docs
    pub(super) len: usize,

    /// TODO: docs
    max_run_id: RunId,
}

impl core::fmt::Debug for RunSummary {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "{{ len: {} }}", self.len)
    }
}

impl Add<Self> for RunSummary {
    type Output = Self;

    #[inline]
    fn add(mut self, rhs: Self) -> Self {
        self += rhs;
        self
    }
}

impl AddAssign<Self> for RunSummary {
    #[inline]
    fn add_assign(&mut self, other: Self) {
        self.len += other.len;
        self.max_run_id += &other.max_run_id;
    }
}

impl Sub<Self> for RunSummary {
    type Output = Self;

    #[inline]
    fn sub(mut self, rhs: Self) -> Self {
        self -= rhs;
        self
    }
}

impl SubAssign<Self> for RunSummary {
    #[inline]
    fn sub_assign(&mut self, other: Self) {
        self.len -= other.len;
        self.max_run_id -= &other.max_run_id;
    }
}

impl Summarize for EditRun {
    type Summary = RunSummary;

    #[inline]
    fn summarize(&self) -> Self::Summary {
        RunSummary {
            len: self.len * (self.is_visible as usize),
            max_run_id: self.run_id.clone(),
        }
    }
}
