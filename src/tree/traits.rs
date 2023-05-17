use core::fmt::Debug;
use core::ops::{Add, AddAssign, Sub, SubAssign};

pub trait Summarize: Debug {
    type Summary: Debug
        + Default
        + Clone
        + for<'a> Add<&'a Self::Summary, Output = Self::Summary>
        + for<'a> Sub<&'a Self::Summary, Output = Self::Summary>
        + for<'a> AddAssign<&'a Self::Summary>
        + for<'a> SubAssign<&'a Self::Summary>
        + PartialEq<Self::Summary>;

    fn summarize(&self) -> Self::Summary;
}

pub trait Leaf: Summarize {}

impl<L: Summarize> Leaf for L {}
