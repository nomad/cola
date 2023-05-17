use core::fmt::Debug;
use core::ops::{Add, AddAssign};

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
