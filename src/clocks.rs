/// TODO: docs
#[derive(Copy, Clone, Default)]
pub struct CharacterClock(u64);

impl core::fmt::Debug for CharacterClock {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "CharacterClock({})", self.0)
    }
}

impl CharacterClock {
    #[inline(always)]
    pub fn new() -> Self {
        Self::default()
    }

    /// TODO: docs
    #[inline]
    pub fn next(&mut self) -> CharacterTimestamp {
        let next = self.0;
        self.0 += 1;
        CharacterTimestamp(next)
    }
}

/// TODO: docs
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct CharacterTimestamp(pub u64);

impl CharacterTimestamp {
    #[inline]
    pub const fn as_u64(&self) -> u64 {
        self.0
    }

    #[inline]
    pub const fn from_u64(ts: u64) -> Self {
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
