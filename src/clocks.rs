/// TODO: docs
#[derive(Copy, Clone, Default)]
pub struct LocalClock(u64);

impl core::fmt::Debug for LocalClock {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "LocalClock({})", self.0)
    }
}

impl LocalClock {
    /// TODO: docs
    #[inline]
    pub fn next(&mut self) -> LocalTimestamp {
        let next = self.0;
        self.0 += 1;
        LocalTimestamp(next)
    }
}

/// TODO: docs
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct LocalTimestamp(u64);

impl LocalTimestamp {
    #[inline]
    pub const fn as_u64(&self) -> u64 {
        self.0
    }

    /// TODO: docs
    pub fn from_u64(ts: u64) -> Self {
        Self(ts)
    }
}

/// TODO: docs
#[derive(Copy, Clone, Default)]
pub(super) struct LamportClock(u64);

impl core::fmt::Debug for LamportClock {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "LamportClock({})", self.0)
    }
}

impl LamportClock {
    /// TODO: docs
    #[inline]
    pub fn next(&mut self) -> LamportTimestamp {
        let next = self.0;
        self.0 += 1;
        LamportTimestamp(next)
    }

    /// TODO: docs
    #[inline]
    pub fn update(&mut self, other: Self) {
        self.0 = self.0.max(other.0) + 1;
    }
}

/// TODO: docs
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct LamportTimestamp(u64);

impl LamportTimestamp {
    #[inline]
    pub const fn as_u64(&self) -> u64 {
        self.0
    }
}
