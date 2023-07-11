use core::hash::BuildHasherDefault;
use std::collections::HashMap;

pub type ReplicaIdMap<T> =
    HashMap<ReplicaId, T, BuildHasherDefault<ReplicaIdHasher>>;

/// A unique[^unique] identifier for a `Replica`.
///
/// Internally this is just a 64-bit integer that is randomly-generated every
/// time a `Replica` is created via the [`new`](crate::Replica::new) method,
/// `clone`d or `Deserialize`d.
///
/// [^unique]: you'd have to have almost [200k peers][table] in the same
/// editing session to reach a one-in-a-billion chance of a single collision,
/// which is more than good enough for the kind of use cases this library is
/// designed for.
///
/// [table]: https://en.wikipedia.org/wiki/Birthday_problem#Probability_table
#[derive(Copy, Clone, PartialEq, PartialOrd, Ord)]
pub struct ReplicaId(u64);

impl core::fmt::Debug for ReplicaId {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "ReplicaId({:x})", self.as_u32())
    }
}

impl From<u64> for ReplicaId {
    #[inline]
    fn from(id: u64) -> Self {
        Self(id)
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
    pub(crate) fn as_u32(self) -> u32 {
        self.0 as u32
    }

    #[inline]
    pub(crate) fn as_u64(self) -> u64 {
        self.0
    }

    /// Returns the "nil" id, i.e. the id whose bytes are all zeros.
    ///
    /// This is used to form the [`EditId`] of the first edit run and should
    /// never be used in any of the following user-generated insertion.
    #[inline]
    pub(crate) const fn zero() -> Self {
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
