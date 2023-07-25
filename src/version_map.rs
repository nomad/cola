use core::cmp::{Ordering, PartialOrd};

use crate::{DeletionTs, Length, ReplicaId, ReplicaIdMap};

pub type DeletionMap = BaseMap<DeletionTs>;

pub type VersionMap = BaseMap<Length>;

/// A struct equivalent to a `HashMap<ReplicaId, T>`, but with the `ReplicaId`
/// of the local `Replica` (and the corresponding `T`) stored separately from
/// the `HashMap` itself.
#[derive(Clone, PartialEq)]
#[cfg_attr(
    any(feature = "encode", feature = "serde"),
    derive(serde::Serialize, serde::Deserialize)
)]
pub struct BaseMap<T> {
    /// The `ReplicaId` of the `Replica` that this map is used in.
    this_id: ReplicaId,

    /// The local value.
    this_value: T,

    /// The values of the remote `Replica`s.
    rest: ReplicaIdMap<T>,
}

impl<T: Copy> BaseMap<T> {
    #[inline]
    pub fn get(&self, replica_id: ReplicaId) -> T
    where
        T: Default,
    {
        if replica_id == self.this_id {
            self.this_value
        } else {
            self.rest.get(&replica_id).copied().unwrap_or_default()
        }
    }

    #[inline]
    pub fn get_mut(&mut self, replica_id: ReplicaId) -> &mut T
    where
        T: Default,
    {
        if replica_id == self.this_id {
            &mut self.this_value
        } else {
            self.rest.entry(replica_id).or_default()
        }
    }

    #[inline]
    pub fn fork(&self, new_id: ReplicaId, restart_at: T) -> Self {
        let mut forked = self.clone();
        forked.fork_in_place(new_id, restart_at);
        forked
    }

    #[inline]
    pub fn fork_in_place(&mut self, new_id: ReplicaId, restart_at: T) {
        self.insert(self.this_id, self.this_value);
        self.this_id = new_id;
        self.this_value = restart_at;
    }

    #[inline]
    pub fn insert(&mut self, replica_id: ReplicaId, value: T) {
        self.rest.insert(replica_id, value);
    }

    #[inline]
    fn iter(&self) -> impl Iterator<Item = (ReplicaId, T)> + '_ {
        let this_entry = core::iter::once((self.this_id, self.this_value));
        this_entry.chain(self.rest.iter().map(|(&id, &value)| (id, value)))
    }

    #[inline]
    pub fn new(this_id: ReplicaId, this_value: T) -> Self {
        Self { this_id, this_value, rest: ReplicaIdMap::default() }
    }

    #[inline]
    pub fn this(&self) -> T {
        self.this_value
    }

    #[inline]
    pub fn this_id(&self) -> ReplicaId {
        self.this_id
    }

    #[inline]
    pub fn this_mut(&mut self) -> &mut T {
        &mut self.this_value
    }
}

impl<T: core::fmt::Debug> core::fmt::Debug for BaseMap<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let this_entry = core::iter::once((&self.this_id, &self.this_value));
        f.debug_map().entries(this_entry.chain(self.rest.iter())).finish()
    }
}

impl PartialOrd for VersionMap {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        fn confirm_left_greater(
            left: &VersionMap,
            right: &VersionMap,
        ) -> bool {
            for (right_id, right_value) in right.iter() {
                let left_value = left.get(right_id);
                if right_value > left_value {
                    return false;
                }
            }

            true
        }

        match self.this().cmp(&other.get(self.this_id)) {
            Ordering::Greater => {
                return if confirm_left_greater(self, other) {
                    Some(Ordering::Greater)
                } else {
                    None
                };
            },

            Ordering::Less => {
                return if confirm_left_greater(other, self) {
                    Some(Ordering::Less)
                } else {
                    None
                };
            },

            Ordering::Equal => {},
        }

        let mut cmp = Ordering::Equal;

        let mut checked = 0;

        for (this_id, this) in &self.rest {
            if let Some(other) = other.rest.get(this_id) {
                match this.cmp(other) {
                    Ordering::Greater => {
                        if cmp == Ordering::Less {
                            return None;
                        } else {
                            cmp = Ordering::Greater;
                        }
                    },

                    Ordering::Less => {
                        if cmp == Ordering::Greater {
                            return None;
                        } else {
                            cmp = Ordering::Less;
                        }
                    },

                    Ordering::Equal => {},
                }
                checked += 1;
            } else if cmp == Ordering::Less {
                return None;
            } else {
                cmp = Ordering::Greater;
            }
        }

        if checked < other.rest.len() {
            if cmp == Ordering::Greater {
                None
            } else {
                Some(Ordering::Less)
            }
        } else {
            debug_assert_eq!(checked, other.rest.len());
            Some(cmp)
        }
    }
}
