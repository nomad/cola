use core::hash::BuildHasherDefault;
use std::collections::HashMap;

pub type ReplicaIdMap<T> =
    HashMap<ReplicaId, T, BuildHasherDefault<ReplicaIdHasher>>;

/// TODO: docs
#[derive(Copy, Clone, PartialEq, PartialOrd, Ord)]
pub struct ReplicaId(u64);

impl core::fmt::Debug for ReplicaId {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "ReplicaId({:x})", self.as_u32())
    }
}

impl core::cmp::Eq for ReplicaId {}

impl core::hash::Hash for ReplicaId {
    #[inline]
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        state.write_u64(self.0);
    }
}

impl ReplicaId {
    #[inline]
    pub fn as_u32(self) -> u32 {
        self.0 as u32
    }

    #[inline]
    pub fn as_u64(self) -> u64 {
        self.0
    }

    /// Creates a new, randomly generated [`ReplicaId`].
    #[inline]
    pub fn new() -> Self {
        Self(rand::random())
    }

    /// Returns the "nil" id, i.e. the id whose bytes are all zeros.
    ///
    /// This is used to form the [`EditId`] of the first edit run and should
    /// never be used in any of the following user-generated insertion.
    #[inline]
    pub const fn zero() -> Self {
        Self(0)
    }
}

#[derive(Default)]
pub struct ReplicaIdHasher(u64);

impl core::hash::Hasher for ReplicaIdHasher {
    fn write(&mut self, _bytes: &[u8]) {
        unreachable!();
    }

    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }

    #[inline]
    fn write_u64(&mut self, i: u64) {
        self.0 = i;
    }
}
