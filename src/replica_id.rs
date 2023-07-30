use core::hash::BuildHasherDefault;
use std::collections::HashMap;

pub type ReplicaIdMap<T> =
    HashMap<ReplicaId, T, BuildHasherDefault<ReplicaIdHasher>>;

pub type ReplicaIdMapValuesMut<'a, T> =
    std::collections::hash_map::ValuesMut<'a, ReplicaId, T>;

/// A unique identifier for a [`Replica`](crate::Replica).
///
/// It's very important that all [`Replica`](crate::Replica)s in the same
/// collaborative session have unique [`ReplicaId`]s as this type is used to
/// distinguish between them when integrating remote edits.
///
/// Guaranteeing uniqueness is up to you.
///
/// If your editing session is not proxied through a server you control you can
/// generate a random `u64` every time a new [`Replica`](crate::Replica) is
/// created and be reasonably[^collisions] sure that there won't be any
/// collisions.
///
/// [^collisions]: you'd have to have almost [200k peers][table] in the same
/// editing session to reach a one-in-a-billion chance of a single collision,
/// which is more than good enough for the kind of use cases this library is
/// designed for.
///
/// [table]: https://en.wikipedia.org/wiki/Birthday_problem#Probability_table
pub type ReplicaId = u64;

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
