use core::cmp::Ord;
use core::cmp::{Ordering, PartialOrd};
use core::fmt::{Debug, Formatter, Result as FmtResult};
use core::ops::{Add, Range as StdRange, RangeBounds, Sub};

use crate::{Length, ReplicaId, ReplicaIdMap};

/// TODO: docs
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct VersionMap {
    #[cfg_attr(feature = "serde", serde(flatten))]
    map: ReplicaIdMap<Length>,
}

impl PartialOrd for VersionMap {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        todo!();
    }
}

impl VersionMap {
    #[inline]
    pub fn insert(&mut self, replica_id: ReplicaId, value: Length) {
        self.map.insert(replica_id, value);
    }

    #[inline]
    pub fn get(&self, replica_id: ReplicaId) -> Option<Length> {
        self.map.get(&replica_id).copied()
    }

    #[inline]
    pub fn get_mut(&mut self, replica_id: ReplicaId) -> Option<&mut Length> {
        self.map.get_mut(&replica_id)
    }

    #[inline]
    pub fn new() -> Self {
        Self { map: ReplicaIdMap::default() }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Range<T> {
    pub start: T,
    pub end: T,
}

impl<T: Debug> Debug for Range<T> {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        write!(f, "{:?}..{:?}", self.start, self.end)
    }
}

impl<T> From<StdRange<T>> for Range<T> {
    #[inline]
    fn from(range: StdRange<T>) -> Self {
        Range { start: range.start, end: range.end }
    }
}

impl<T> From<Range<T>> for StdRange<T> {
    #[inline]
    fn from(range: Range<T>) -> Self {
        StdRange { start: range.start, end: range.end }
    }
}

impl<T: Sub<T, Output = T> + Copy> Sub<T> for Range<T> {
    type Output = Range<T>;

    #[inline]
    fn sub(self, value: T) -> Self::Output {
        Range { start: self.start - value, end: self.end - value }
    }
}

impl<T: Add<T, Output = T> + Copy> Add<T> for Range<T> {
    type Output = Range<T>;

    #[inline]
    fn add(self, value: T) -> Self::Output {
        Range { start: self.start + value, end: self.end + value }
    }
}

impl<T> Range<T> {
    #[inline]
    pub fn len(&self) -> T
    where
        T: Sub<T, Output = T> + Copy,
    {
        self.end - self.start
    }
}

pub(crate) trait RangeExt<T> {
    fn contains_range(&self, range: Range<T>) -> bool;
}

impl<T: Ord> RangeExt<T> for StdRange<T> {
    #[inline]
    fn contains_range(&self, other: Range<T>) -> bool {
        self.start <= other.start && self.end >= other.end
    }
}

/// TODO: docs
#[inline]
pub(crate) fn get_two_mut<T>(
    slice: &mut [T],
    first_idx: usize,
    second_idx: usize,
) -> (&mut T, &mut T) {
    debug_assert!(first_idx != second_idx);

    if first_idx < second_idx {
        debug_assert!(second_idx < slice.len());
        let split_at = first_idx + 1;
        let (first, second) = slice.split_at_mut(split_at);
        (&mut first[first_idx], &mut second[second_idx - split_at])
    } else {
        debug_assert!(first_idx < slice.len());
        let split_at = second_idx + 1;
        let (first, second) = slice.split_at_mut(split_at);
        (&mut second[first_idx - split_at], &mut first[second_idx])
    }
}

/// TODO: docs
#[inline]
pub(crate) fn insert_in_slice<T>(slice: &mut [T], elem: T, at_offset: usize) {
    debug_assert!(at_offset < slice.len());
    slice[at_offset..].rotate_right(1);
    slice[at_offset] = elem;
}

/// TODO: docs
#[inline(always)]
pub(crate) fn range_bounds_to_start_end<R>(
    range: R,
    lo: Length,
    hi: Length,
) -> (Length, Length)
where
    R: RangeBounds<Length>,
{
    use core::ops::Bound;

    let start = match range.start_bound() {
        Bound::Included(&n) => n,
        Bound::Excluded(&n) => n + 1,
        Bound::Unbounded => lo,
    };

    let end = match range.end_bound() {
        Bound::Included(&n) => n + 1,
        Bound::Excluded(&n) => n,
        Bound::Unbounded => hi,
    };

    (start, end)
}
