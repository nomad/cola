/// TODO: docs
#[derive(Copy, Clone, Default)]
pub struct LocalClock(u32);

impl core::fmt::Debug for LocalClock {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "LocalClock({})", self.0)
    }
}

impl LocalClock {
    #[inline(always)]
    pub fn new() -> Self {
        Self::default()
    }

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
pub struct LocalTimestamp(u32);

impl LocalTimestamp {
    #[inline]
    pub const fn as_u32(&self) -> u32 {
        self.0
    }

    #[inline]
    pub const fn from_u32(ts: u32) -> Self {
        Self(ts)
    }
}

/// TODO: docs
#[derive(Copy, Clone, Default)]
pub struct LamportClock(u64);

impl core::fmt::Debug for LamportClock {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "LamportClock({})", self.0)
    }
}

impl LamportClock {
    #[inline(always)]
    pub fn new() -> Self {
        Self::default()
    }

    /// TODO: docs
    #[inline]
    pub fn next(&mut self) -> LamportTimestamp {
        let next = self.0;
        self.0 += 1;
        LamportTimestamp(next)
    }

    /// TODO: docs
    #[inline]
    pub fn update(&mut self, other: LamportTimestamp) -> LamportTimestamp {
        self.0 = self.0.max(other.0) + 1;
        LamportTimestamp(self.0)
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
