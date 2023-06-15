use core::cmp::Ordering;
use core::ops::Range;

use crate::*;

/// TODO: docs
#[derive(Clone, PartialEq)]
pub struct InsertionRun {
    /// TODO: docs
    id: InsertionId,

    /// TODO: docs
    inserted_at: Anchor,

    /// TODO: docs
    lamport_ts: LamportTimestamp,

    /// TODO: docs
    len: Length,

    /// TODO: docs
    is_visible: bool,

    /// TODO: docs
    is_last_run: bool,
}

impl core::fmt::Debug for InsertionRun {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(
            f,
            "{:?} L({}) |> {:?}, {} {}",
            self.id,
            self.lamport_ts.as_u64(),
            self.inserted_at,
            self.len,
            if self.is_visible { "✔️" } else { "🪦" },
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
        match other.lamport_ts.cmp(&self.lamport_ts) {
            Ordering::Equal => Some(self.replica_id().cmp(other.replica_id())),

            other => Some(other),
        }
    }
}

impl InsertionRun {
    /// TODO: docs
    #[inline(always)]
    pub fn bisect_by_local_run(
        &mut self,
        replica_id: ReplicaId,
        run_len: Length,
        at_offset: Length,
        local_clock: &mut LocalClock,
        lamport_clock: &mut LamportClock,
        //id_registry: &mut RunIdRegistry,
    ) -> (Option<Self>, Option<Self>) {
        // When a new insertion extends a previous insertion neither the local
        // nor the lamport clocks are increased. In a way it's like we're
        // pretending that the original insertion always ended in the insertion
        // we're adding now.

        self.bisect(
            &replica_id,
            || InsertionId::new(replica_id, local_clock.next()),
            || lamport_clock.next(),
            run_len,
            at_offset,
            //id_registry,
        )
    }

    /// TODO: docs
    #[inline(always)]
    pub fn bisect_by_remote_run(
        &mut self,
        id: InsertionId,
        run_len: Length,
        at_offset: Length,
        lamport_ts: LamportTimestamp,
        //id_registry: &mut RunIdRegistry,
    ) -> (Option<Self>, Option<Self>) {
        self.bisect(
            &id.replica_id().clone(),
            || id,
            || lamport_ts,
            run_len,
            at_offset,
            //id_registry,
        )
    }

    /// TODO: docs
    pub fn bisect(
        &mut self,
        replica_id: &ReplicaId,
        get_insertion_id: impl FnOnce() -> InsertionId,
        get_lamport_ts: impl FnOnce() -> LamportTimestamp,
        run_len: Length,
        at_offset: Length,
        //id_registry: &mut RunIdRegistry,
    ) -> (Option<Self>, Option<Self>) {
        debug_assert!(at_offset <= self.len);

        if replica_id == self.replica_id()
            && at_offset == self.len
            && self.is_last_run
        {
            self.len += run_len;

            // id_registry.extend_insertion(self.insertion_id(), run_len);

            (None, None)
        } else if at_offset == 0 {
            let run = Self {
                id: get_insertion_id(),
                inserted_at: Anchor::origin(),
                lamport_ts: get_lamport_ts(),
                len: run_len,
                is_visible: true,
                is_last_run: true,
            };

            // id_registry.add_insertion(run.id, run.len, run.id.clone());

            let this = core::mem::replace(self, run);

            (Some(this), None)
        } else if at_offset == self.len {
            let new_run = Self {
                id: get_insertion_id(),
                inserted_at: Anchor::new(self.id.clone(), self.len),
                lamport_ts: get_lamport_ts(),
                len: run_len,
                is_visible: true,
                is_last_run: true,
            };

            (Some(new_run), None)
        } else {
            let split_run = self.split(at_offset /* id_registry */);

            let new_run = Self {
                id: get_insertion_id(),
                inserted_at: Anchor::new(self.id.clone(), self.len),
                lamport_ts: get_lamport_ts(),
                len: run_len,
                is_visible: true,
                is_last_run: true,
            };

            //id_registry.add_insertion(
            //    new_run.id.clone(),
            //    new_run.len,
            //    new_run.run_id.clone(),
            //);

            // id_registry.split_insertion();

            (Some(new_run), Some(split_run))
        }
    }

    #[inline(always)]
    pub fn anchor(&self) -> &Anchor {
        &self.inserted_at
    }

    #[inline(always)]
    pub fn id(&self) -> &InsertionId {
        &self.id
    }

    #[inline(always)]
    pub fn replica_id(&self) -> &ReplicaId {
        &self.id.replica_id
    }

    #[inline]
    pub fn delete(&mut self) {
        self.is_visible = false;
    }

    #[inline]
    pub fn delete_from(
        &mut self,
        offset: Length,
        //id_registry: &mut RunIdRegistry,
    ) -> Option<Self> {
        if offset == 0 {
            self.is_visible = false;
            None
        } else if offset < self.len {
            let mut del = self.split(offset /* , id_registry */);
            del.is_visible = false;
            Some(del)
        } else {
            None
        }
    }

    #[inline]
    pub fn delete_range(
        &mut self,
        Range { start, end }: Range<Length>,
        //id_registry: &mut RunIdRegistry,
    ) -> (Option<Self>, Option<Self>) {
        debug_assert!(start <= end);

        if start == end {
            (None, None)
        } else if start == 0 {
            (self.delete_up_to(end /* id_registry */), None)
        } else if end >= self.len {
            (self.delete_from(start /* id_registry */), None)
        } else {
            let rest = self.split(end /* id_registry */);
            let mut deleted = self.split(start /* id_registry */);
            deleted.is_visible = false;
            (Some(deleted), Some(rest))
        }
    }

    #[inline]
    pub fn delete_up_to(
        &mut self,
        offset: Length,
        // id_registry: &mut RunIdRegistry,
    ) -> Option<Self> {
        if offset == 0 {
            None
        } else if offset < self.len {
            let rest = self.split(offset /* id_registry */);
            self.is_visible = false;
            Some(rest)
        } else {
            self.is_visible = false;
            None
        }
    }

    /// TODO: docs
    fn split(
        &mut self,
        at_offset: Length,
        // id_registry: &mut RunIdRegistry,
    ) -> Self {
        debug_assert!(at_offset > 0 && at_offset < self.len);

        let mut split = self.clone();

        split.len = self.len - at_offset;

        split.is_last_run = self.is_last_run;

        self.len = at_offset;

        self.is_last_run = false;

        // id_registry.split_insertion(
        //     self.id,
        //     at_offset,
        //     split_run_id,
        // );

        split
    }

    /// TODO: docs
    pub fn origin(
        id: InsertionId,
        lamport_ts: LamportTimestamp,
        len: Length,
    ) -> Self {
        debug_assert_eq!(0, id.local_ts.as_u64());
        debug_assert_eq!(0, lamport_ts.as_u64());

        Self {
            id,
            inserted_at: Anchor::origin(),
            lamport_ts,
            len,
            is_visible: true,
            is_last_run: true,
        }
    }

    #[inline(always)]
    pub fn len(&self) -> Length {
        self.len
    }
}

/// TODO: docs
///
/// TODO: optimize the space, we don't need 192 bits. We'll usually need ~8
/// bits for the replica id and ~16 for the local ts, i.e. ~ 25-30 bits
#[derive(Clone, PartialEq)]
pub struct InsertionId {
    /// The id of the replica that originally created this insertion.
    replica_id: ReplicaId,

    /// The local timestamp of the replica when this insertion was created.
    local_ts: LocalTimestamp,
}

impl core::fmt::Debug for InsertionId {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "{:x}.{}", self.replica_id.as_u32(), self.local_ts.as_u64())
    }
}

impl InsertionId {
    #[inline(always)]
    pub fn replica_id(&self) -> &ReplicaId {
        &self.replica_id
    }

    #[inline(always)]
    pub fn local_ts(&self) -> LocalTimestamp {
        self.local_ts
    }

    #[inline(always)]
    pub fn new(replica_id: ReplicaId, local_ts: LocalTimestamp) -> Self {
        Self { replica_id, local_ts }
    }
}

/// TODO: docs
#[derive(Clone, PartialEq)]
pub struct Anchor {
    /// The id of the insertion this anchor is in.
    insertion_id: InsertionId,

    /// An offset inside the insertion used to track anchor over time.
    ///
    /// When the position we want to track falls between 2 distinct insertion
    /// runs the left run will be used as the anchor.
    ///
    /// For example let's say a buffer "ab" is made up of 2 distinct runs: "a"
    /// with [`InsertionId`] 1.0 and "b" with id 1.1. If we want to track the
    /// position between the 'a' and the 'b' the anchor we create is 1.0 @ 1
    /// -- read as "1.0 at offset 1" -- and **not** 1.1 @ 0.
    ///
    /// It follows that this field is *never zero*, *except* when we want
    /// to anchor at the start of the document. In this case there's no such
    /// "insertion run to the left" to anchor to and we use a special anchor
    /// called "origin" returned by [`Anchor::origin()`](Anchor::origin()).
    ///
    /// It also follows that the maximum value this field can have is the
    /// length of the insertion at the time when the anchor was created.
    offset: Length,
}

impl core::fmt::Debug for Anchor {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        if self == &Self::origin() {
            write!(f, "origin")
        } else {
            write!(f, "{:?} @ {}", self.insertion_id, self.offset)
        }
    }
}

impl Anchor {
    #[inline(always)]
    pub fn insertion_id(&self) -> &InsertionId {
        &self.insertion_id
    }

    #[inline(always)]
    pub fn offset(&self) -> Length {
        self.offset
    }

    /// A special value used to create an anchor at the start of the document.
    #[inline]
    pub const fn origin() -> Self {
        Self {
            insertion_id: InsertionId {
                replica_id: ReplicaId::zero(),
                local_ts: LocalTimestamp::from_u64(0),
            },
            offset: 0,
        }
    }

    #[inline(always)]
    pub fn new(insertion_id: InsertionId, offset: Length) -> Self {
        Self { insertion_id, offset }
    }
}

impl Summarize for InsertionRun {
    type Length = Length;

    #[inline]
    fn summarize(&self) -> Self::Length {
        self.len * (self.is_visible as Length)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
}
