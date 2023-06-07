use core::ops::{Add, AddAssign, Sub, SubAssign};

use crate::node::Metric;
use crate::{EditRun, RunSummary};

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ByteMetric(pub(super) usize);

impl Add<Self> for ByteMetric {
    type Output = Self;

    #[inline]
    fn add(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }
}

impl Sub for ByteMetric {
    type Output = Self;

    #[inline]
    fn sub(self, other: Self) -> Self {
        Self(self.0 - other.0)
    }
}

impl AddAssign for ByteMetric {
    #[inline]
    fn add_assign(&mut self, other: Self) {
        self.0 += other.0
    }
}

impl SubAssign for ByteMetric {
    #[inline]
    fn sub_assign(&mut self, other: Self) {
        self.0 -= other.0
    }
}

impl Metric<EditRun> for ByteMetric {
    #[inline]
    fn zero() -> Self {
        Self(0)
    }

    #[inline]
    fn measure_leaf(fragment: &EditRun) -> Self {
        Self(fragment.len())
    }

    #[inline]
    fn measure_summary(summary: &RunSummary) -> Self {
        Self(summary.len)
    }
}
