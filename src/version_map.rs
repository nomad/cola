use core::cmp::{Ordering, PartialOrd};

use crate::{DeletionTs, Length, ReplicaId, ReplicaIdMap};

pub type DeletionMap = BaseMap<DeletionTs>;

pub type VersionMap = BaseMap<Length>;

/// A struct equivalent to a `HashMap<ReplicaId, T>`, but with the `ReplicaId`
/// of the local `Replica` (and the corresponding `T`) stored separately from
/// the `HashMap` itself.
#[derive(Clone, PartialEq, Eq)]
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
        #[inline]
        fn update_order<I>(iter: I, mut order: Ordering) -> Option<Ordering>
        where
            I: Iterator<Item = (Length, Length)>,
        {
            for (this_value, other_value) in iter {
                match this_value.cmp(&other_value) {
                    Ordering::Greater => {
                        if order == Ordering::Less {
                            return None;
                        }
                        order = Ordering::Greater;
                    },

                    Ordering::Less => {
                        if order == Ordering::Greater {
                            return None;
                        }
                        order = Ordering::Less;
                    },

                    Ordering::Equal => {},
                }
            }

            Some(order)
        }

        let mut order = Ordering::Equal;

        let iter = self.iter().filter(|(_, value)| *value > 0).map(
            |(this_id, this_value)| {
                let other_value = other.get(this_id);
                (this_value, other_value)
            },
        );

        order = update_order(iter, order)?;

        let iter = other.iter().filter(|(_, value)| *value > 0).map(
            |(other_id, other_value)| {
                let this_value = self.get(other_id);
                (this_value, other_value)
            },
        );

        update_order(iter, order)
    }
}

#[cfg(feature = "encode")]
mod encode {
    use super::*;
    use crate::encode::{Decode, Encode, IntDecodeError};

    impl<T: Encode> Encode for BaseMap<T> {
        #[inline]
        fn encode(&self, buf: &mut Vec<u8>) {
            self.this_id.encode(buf);
            self.this_value.encode(buf);
            (self.rest.len() as u64).encode(buf);
            for (id, value) in self.rest.iter() {
                id.encode(buf);
                value.encode(buf);
            }
        }
    }

    pub(crate) enum BaseMapDecodeError<T: Decode> {
        Key(IntDecodeError),
        Value(T::Error),
    }

    impl<T: Decode> From<IntDecodeError> for BaseMapDecodeError<T> {
        #[inline(always)]
        fn from(err: IntDecodeError) -> Self {
            Self::Key(err)
        }
    }

    impl<T: Decode> core::fmt::Display for BaseMapDecodeError<T> {
        #[inline]
        fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
            let err: &dyn core::fmt::Display = match self {
                Self::Key(err) => err,
                Self::Value(err) => err,
            };

            write!(
                f,
                "BaseMap<{:?}>: couldn't be decoded: {err}",
                core::any::type_name::<T>()
            )
        }
    }

    impl<T: Decode<Value = T>> Decode for BaseMap<T> {
        type Value = Self;

        type Error = BaseMapDecodeError<T>;

        #[inline]
        fn decode(buf: &[u8]) -> Result<(Self::Value, &[u8]), Self::Error> {
            let (this_id, buf) = ReplicaId::decode(buf)?;

            let (this_value, buf) =
                T::decode(buf).map_err(BaseMapDecodeError::Value)?;

            let (rest_len, mut buf) = u64::decode(buf)?;

            let mut rest = ReplicaIdMap::default();

            for _ in 0..rest_len {
                let (id, new_buf) = ReplicaId::decode(buf)?;

                let (value, new_buf) =
                    T::decode(new_buf).map_err(BaseMapDecodeError::Value)?;

                rest.insert(id, value);

                buf = new_buf;
            }

            let this = Self { this_id, this_value, rest };

            Ok((this, buf))
        }
    }
}

#[cfg(feature = "serde")]
mod serde {
    crate::encode::impl_serialize!(super::DeletionMap);
    crate::encode::impl_deserialize!(super::DeletionMap);

    crate::encode::impl_serialize!(super::VersionMap);
    crate::encode::impl_deserialize!(super::VersionMap);
}
