/// TODO: docs
#[derive(Copy, Clone, Debug, Default)]
pub(super) struct LocalClock(u64);

impl LocalClock {
    /// TODO: docs
    #[inline]
    pub(super) fn next(&mut self) -> LocalTimestamp {
        let next = self.0;
        self.0 += 1;
        LocalTimestamp(next)
    }
}

/// TODO: docs
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(super) struct LocalTimestamp(u64);

/// TODO: docs
#[derive(Copy, Clone, Debug, Default)]
pub(super) struct LamportClock(u64);

impl LamportClock {
    /// TODO: docs
    #[inline]
    pub(super) fn next(&mut self) -> LamportTimestamp {
        let next = self.0;
        self.0 += 1;
        LamportTimestamp(next)
    }

    /// TODO: docs
    #[inline]
    pub(super) fn update(&mut self, other: Self) {
        self.0 = self.0.max(other.0) + 1;
    }
}

/// TODO: docs
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct LamportTimestamp(u64);
