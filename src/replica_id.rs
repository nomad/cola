use core::hash::BuildHasherDefault;
use std::collections::HashMap;

pub type ReplicaIdMap<T> =
    HashMap<ReplicaId, T, BuildHasherDefault<ReplicaIdHasher>>;

pub type ReplicaIdMapValuesMut<'a, T> =
    std::collections::hash_map::ValuesMut<'a, ReplicaId, T>;

/// A unique identifier for a `Replica`.
///
/// Internally this is a newtype around 64-bit integer and can be created via
/// its [`From<u64>`](ReplicaId#impl-From<u64>-for-ReplicaId) implementation.
///
/// It's very important that all `Replica`s in the same collaborative session
/// have unique `ReplicaId`s as this type is used to distinguish between them
/// when [`merge`](crate::Replica::merge)ing edits.
///
/// Guaranteeing uniqueness is up to you.
///
/// If the session is proxied through a server you control you can use that to
/// centrally assign ids and increment a counter every time a new peer joins
/// the session.
///
/// If not, you can generate a random `u64` every time a new `Replica` is
/// created and be reasonably[^collisions] sure that there won't be any
/// collisions.
///
/// [^collisions]: you'd have to have almost [200k peers][table] in the same
/// editing session to reach a one-in-a-billion chance of a single collision,
/// which is more than good enough for the kind of use cases this library is
/// designed for.
///
/// [table]: https://en.wikipedia.org/wiki/Birthday_problem#Probability_table
#[derive(Copy, Clone, PartialEq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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
