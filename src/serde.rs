use serde::{de, ser};

use crate::{CrdtEdit, Replica};

impl ser::Serialize for Replica {
    #[inline]
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        todo!();
    }
}

impl<'de> de::Deserialize<'de> for Replica {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        todo!();
    }
}

impl ser::Serialize for CrdtEdit {
    #[inline]
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        todo!();
    }
}

impl<'de> de::Deserialize<'de> for CrdtEdit {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        todo!();
    }
}
