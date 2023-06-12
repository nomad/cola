use core::fmt::Debug;
use core::ops::{Add, AddAssign, Sub, SubAssign};

use crate::{InsertionRun, RunSummary};

/// TODO: docs
pub type Length = usize;

/// TODO: docs
pub trait Metric {
    /// TODO: docs
    fn len(s: &str) -> Length;
}

/// TODO: docs
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ByteMetric(pub(crate) usize);

impl Add<Self> for ByteMetric {
    type Output = Self;

    #[inline(always)]
    fn add(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }
}

impl Sub for ByteMetric {
    type Output = Self;

    #[inline(always)]
    fn sub(self, other: Self) -> Self {
        Self(self.0 - other.0)
    }
}

impl AddAssign for ByteMetric {
    #[inline(always)]
    fn add_assign(&mut self, other: Self) {
        self.0 += other.0
    }
}

impl SubAssign for ByteMetric {
    #[inline(always)]
    fn sub_assign(&mut self, other: Self) {
        self.0 -= other.0
    }
}

impl Metric for ByteMetric {
    #[inline(always)]
    fn len(s: &str) -> Length {
        s.len()
    }
}

impl crate::btree::Metric<InsertionRun> for Length {
    #[inline]
    fn zero() -> Self {
        0
    }

    #[inline]
    fn measure_leaf(run: &InsertionRun) -> Self {
        run.len()
    }

    #[inline]
    fn measure_summary(summary: &RunSummary) -> Self {
        summary.len()
    }
}
