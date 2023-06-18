use core::cmp::Ordering;
use core::ops::Range;

use crate::*;

/// TODO: docs
#[derive(Clone, PartialEq)]
pub struct InsertionRun {
    /// TODO: docs
    inserted_at: Anchor,

    /// TODO: docs
    inserted_by: ReplicaId,

    /// TODO: docs
    character_range: Range<CharacterTimestamp>,

    /// TODO: docs
    lamport_ts: LamportTimestamp,

    /// TODO: docs
    is_deleted: bool,
}

impl core::fmt::Debug for InsertionRun {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(
            f,
            "{:x}.{:?} L({}) |@ {:?}{}",
            self.inserted_by.as_u32(),
            self.character_range,
            self.lamport_ts.as_u64(),
            self.inserted_at,
            if self.is_deleted { " 🪦" } else { "" },
        )
    }
}

/// This implementation is guaranteed to never return `Some(Ordering::Equal)`.
impl PartialOrd for InsertionRun {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        // If the two runs were inserted at different positions they're totally
        // unrelated and we can't compare them.
        if self.inserted_at != other.inserted_at {
            return None;
        };

        // If they have the same anchor we first sort descending on lamport
        // timestamsps, and if those are also the same we use the replica id as
        // a last tie breaker (here we sort ascending on replica ids but that's
        // totally arbitrary).
        Some(match other.lamport_ts.cmp(&self.lamport_ts) {
            Ordering::Equal => self.replica_id().cmp(&other.replica_id()),
            other => other,
        })
    }
}

impl InsertionRun {
    #[inline(always)]
    pub fn anchor(&self) -> Anchor {
        self.inserted_at.clone()
    }

    #[inline(always)]
    pub fn end(&self) -> CharacterTimestamp {
        self.range().end
    }

    #[inline(always)]
    pub fn extend(&mut self, extend_by: u64) {
        self.character_range.end += extend_by;
    }

    #[inline]
    pub fn delete(&mut self) {
        self.is_deleted = true;
    }

    #[inline]
    pub fn delete_from(
        &mut self,
        offset: Length,
        //id_registry: &mut RunIdRegistry,
    ) -> Option<Self> {
        todo!()
        //if offset == 0 {
        //    self.is_visible = false;
        //    None
        //} else if offset < self.len() {
        //    let mut del = self.split(offset /* , id_registry */);
        //    del.is_visible = false;
        //    Some(del)
        //} else {
        //    None
        //}
    }

    #[inline]
    pub fn delete_range(
        &mut self,
        Range { start, end }: Range<Length>,
        //id_registry: &mut RunIdRegistry,
    ) -> (Option<Self>, Option<Self>) {
        debug_assert!(start <= end);

        todo!();

        //if start == end {
        //    (None, None)
        //} else if start == 0 {
        //    (self.delete_up_to(end /* id_registry */), None)
        //} else if end >= self.len() {
        //    (self.delete_from(start /* id_registry */), None)
        //} else {
        //    let rest = self.split(end /* id_registry */);
        //    let mut deleted = self.split(start /* id_registry */);
        //    deleted.is_visible = false;
        //    (Some(deleted), Some(rest))
        //}
    }

    #[inline]
    pub fn delete_up_to(
        &mut self,
        offset: Length,
        // id_registry: &mut RunIdRegistry,
    ) -> Option<Self> {
        todo!()
        //if offset == 0 {
        //    None
        //} else if offset < self.len() {
        //    let rest = self.split(offset /* id_registry */);
        //    self.is_visible = false;
        //    Some(rest)
        //} else {
        //    self.is_visible = false;
        //    None
        //}
    }

    #[inline(always)]
    pub fn lamport_ts(&self) -> LamportTimestamp {
        self.lamport_ts
    }

    /// TODO: docs
    #[inline]
    pub fn len(&self) -> u64 {
        self.end() - self.start()
    }

    /// TODO: docs
    #[inline]
    pub fn new(
        inserted_at: Anchor,
        inserted_by: ReplicaId,
        character_range: Range<CharacterTimestamp>,
        lamport_ts: LamportTimestamp,
    ) -> Self {
        Self {
            inserted_at,
            inserted_by,
            character_range,
            lamport_ts,
            is_deleted: false,
        }
    }

    #[inline(always)]
    pub fn range(&self) -> &Range<CharacterTimestamp> {
        &self.character_range
    }

    #[inline(always)]
    pub fn replica_id(&self) -> ReplicaId {
        self.inserted_by
    }

    /// TODO: docs
    pub fn split(&mut self, at_offset: u64) -> Option<Self> {
        if at_offset == self.len() || at_offset == 0 {
            None
        } else {
            let mut split = self.clone();
            split.character_range.start += at_offset as u64;
            self.character_range.end = split.character_range.start;
            Some(split)
        }
    }

    #[inline(always)]
    pub fn start(&self) -> CharacterTimestamp {
        self.range().start
    }
}

/// TODO: docs
#[derive(Clone, PartialEq)]
pub struct Anchor {
    /// TODO: docs
    replica_id: ReplicaId,

    /// TODO: docs
    offset: CharacterTimestamp,
}

impl core::fmt::Debug for Anchor {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        if self == &Self::origin() {
            write!(f, "origin")
        } else {
            write!(f, "{:x}.{}", self.replica_id.as_u32(), self.offset)
        }
    }
}

impl Anchor {
    #[inline(always)]
    pub fn new(replica_id: ReplicaId, offset: CharacterTimestamp) -> Self {
        Self { replica_id, offset }
    }

    #[inline(always)]
    pub fn offset(&self) -> u64 {
        self.offset
    }

    /// A special value used to create an anchor at the start of the document.
    #[inline]
    pub const fn origin() -> Self {
        Self { replica_id: ReplicaId::zero(), offset: 0 }
    }

    #[inline(always)]
    pub fn replica_id(&self) -> ReplicaId {
        self.replica_id
    }
}

impl Summary for u64 {
    #[inline]
    fn empty() -> Self {
        0
    }
}

impl Summarize for InsertionRun {
    type Summary = u64;

    #[inline]
    fn summarize(&self) -> Self::Summary {
        self.len() * (!self.is_deleted as u64)
    }
}

impl gtree2::Metric<u64> for u64 {
    #[inline]
    fn zero() -> Self {
        0
    }

    #[inline]
    fn measure(this: &Self) -> Self {
        *this
    }
}

#[cfg(test)]
mod tests {
    use super::*;
}
