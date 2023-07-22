use core::cmp::{Ordering, PartialOrd};

use crate::{DeletionTs, Length, ReplicaId, ReplicaIdMap};

pub type DeletionMap = BaseMap<DeletionTs>;

pub type VersionMap = BaseMap<Length>;

impl PartialOrd for VersionMap {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        fn confirm_left_greater(
            left: &VersionMap,
            right: &VersionMap,
        ) -> bool {
            let mut checked = 0;

            if let Some(&right_ts) = right.rest.get(&left.this_id) {
                if right_ts > left.this() {
                    return false;
                }
                checked += 1;
            }

            for (&id, &left_ts) in &left.rest {
                if let Some(right_ts) = right.get_opt(id) {
                    if right_ts > left_ts {
                        return false;
                    }
                    checked += 1;
                }
            }

            checked == right.rest.len()
        }

        match self.rest.len().cmp(&other.rest.len()) {
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

        for (id, this_ts) in &self.rest {
            if let Some(other_ts) = other.rest.get(id) {
                match this_ts.cmp(other_ts) {
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

#[derive(Clone, PartialEq)]
#[cfg_attr(
    any(feature = "encode", feature = "serde"),
    derive(serde::Serialize, serde::Deserialize)
)]
pub struct BaseMap<T> {
    /// TODO: docs
    this_id: ReplicaId,

    /// TODO: docs
    this_value: T,

    /// TODO: docs
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
    pub fn get_opt(&self, replica_id: ReplicaId) -> Option<T> {
        if replica_id == self.this_id {
            Some(self.this_value)
        } else {
            self.rest.get(&replica_id).copied()
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
    pub fn fork_in_place(&mut self, new_id: ReplicaId, restart_at: T) {
        self.insert(self.this_id, self.this_value);
        self.this_id = new_id;
        self.this_value = restart_at;
    }

    #[inline]
    pub fn fork(&self, new_id: ReplicaId, restart_at: T) -> Self {
        let mut forked = self.clone();
        forked.fork_in_place(new_id, restart_at);
        forked
    }

    #[inline]
    pub fn insert(&mut self, replica_id: ReplicaId, value: T) {
        self.rest.insert(replica_id, value);
    }

    #[inline]
    pub fn new(this_id: ReplicaId, this_value: T) -> Self {
        Self { this_id, this_value, rest: ReplicaIdMap::default() }
    }

    #[inline]
    pub fn this_id(&self) -> ReplicaId {
        self.this_id
    }

    #[inline]
    pub fn this(&self) -> T {
        self.this_value
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
