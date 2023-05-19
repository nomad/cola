use core::fmt::Debug;
use core::ops::{Add, AddAssign, Sub, SubAssign};

pub trait Summarize: Debug {
    type Summary: Debug
        + Default
        + Copy
        + Add<Self::Summary, Output = Self::Summary>
        + AddAssign<Self::Summary>
        + PartialEq<Self::Summary>;

    fn summarize(&self) -> Self::Summary;
}

pub trait Leaf: Summarize {}

impl<L: Summarize> Leaf for L {}

pub trait Metric<L: Summarize + ?Sized>:
    Debug
    + Copy
    + Ord
    + Add<Self, Output = Self>
    + Sub<Self, Output = Self>
    + AddAssign<Self>
    + SubAssign<Self>
{
    /// The identity element of this metric with respect to addition.
    ///
    /// Given an implementor `M` of this trait, for all instances `m` of `M`
    /// it should hold `m == m + M::zero()`.
    fn zero() -> Self;

    /// Returns the measure of the summary according to this metric.
    fn measure(summary: &L::Summary) -> Self;
}
