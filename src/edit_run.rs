use alloc::rc::Rc;
use core::ops::{Add, AddAssign, Range, Sub, SubAssign};

use crate::node::Summarize;
use crate::*;

/// TODO: docs
#[derive(Clone)]
pub struct EditRun {
    /// TODO: docs
    edit_id: InsertionId,

    /// TODO: docs
    insertion_id: InsertionAnchor,

    /// TODO: docs
    run_id: RunId,

    /// TODO: docs
    next_run_id: RunId,

    /// TODO: docs
    lamport_ts: LamportTimestamp,

    /// TODO: docs
    len: usize,

    /// TODO: docs
    is_visible: bool,
}

impl core::fmt::Debug for EditRun {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(
            f,
            "{:?} L({}) |> {:?}, {:?} {} {}",
            self.edit_id,
            self.lamport_ts.as_u64(),
            self.insertion_id,
            self.run_id,
            self.len,
            if self.is_visible { "✔️" } else { "🪦" },
        )
    }
}

impl EditRun {
    #[inline]
    pub fn delete(&mut self) {
        self.is_visible = false;
    }

    #[inline]
    pub fn delete_from(&mut self, offset: usize) -> Option<Self> {
        if offset == 0 {
            self.is_visible = false;
            None
        } else {
            self.split(offset).map(|mut del| {
                del.is_visible = false;
                del
            })
        }
    }

    #[inline]
    pub fn delete_range(
        &mut self,
        Range { start, end }: Range<usize>,
    ) -> (Option<Self>, Option<Self>) {
        debug_assert!(start <= end);

        if start == 0 {
            (self.delete_up_to(end), None)
        } else if end >= self.len {
            (self.delete_from(start), None)
        } else {
            let rest = self.split(end);

            let deleted = self.split(start).map(|mut del| {
                del.is_visible = false;
                del
            });

            (deleted, rest)
        }
    }

    #[inline]
    pub fn delete_up_to(&mut self, offset: usize) -> Option<Self> {
        if offset == 0 {
            None
        } else {
            let rest = self.split(offset);
            self.is_visible = false;
            rest
        }
    }

    /// TODO: docs
    pub fn insert(
        &mut self,
        edit_id: InsertionId,
        lamport_ts: LamportTimestamp,
        at_offset: usize,
        len: usize,
        id_registry: &mut RunIdRegistry,
    ) -> (Self, Option<Self>) {
        let insertion_id =
            InsertionAnchor { inside_of: self.edit_id, at_offset };

        // The new run starts at the beginning of this run => swap this run w/
        // the new one and return self.
        if at_offset == 0 {
            let run_id = RunId::between(&RunId::zero(), &self.run_id);

            let new_run = Self {
                edit_id,
                insertion_id,
                run_id: run_id.clone(),
                next_run_id: self.run_id.clone(),
                lamport_ts,
                len,
                is_visible: true,
            };

            id_registry.add_insertion(edit_id, len, run_id);

            let this = core::mem::replace(self, new_run);

            (this, None)
        }
        // The new run starts at the end of this run.
        else if at_offset == self.len {
            let run_id = RunId::between(&self.run_id, &self.next_run_id);

            let next_run_id = self.next_run_id.clone();

            self.next_run_id = run_id.clone();

            let new_run = Self {
                edit_id,
                insertion_id,
                run_id: run_id.clone(),
                next_run_id,
                lamport_ts,
                len,
                is_visible: true,
            };

            id_registry.add_insertion(edit_id, len, run_id);

            (new_run, None)
        }
        // The new run splits this run.
        else {
            let split_run_id = RunId::between(&self.run_id, &self.next_run_id);

            let new_run_id = RunId::between(&self.run_id, &split_run_id);

            let new_run = Self {
                edit_id,
                insertion_id,
                run_id: new_run_id.clone(),
                next_run_id: split_run_id.clone(),
                lamport_ts,
                len,
                is_visible: true,
            };

            id_registry.add_insertion(edit_id, len, new_run_id.clone());

            let old_next_run_id =
                core::mem::replace(&mut self.next_run_id, new_run_id);

            let split_run = Self {
                edit_id: self.edit_id,
                insertion_id: self.insertion_id,
                run_id: split_run_id.clone(),
                next_run_id: old_next_run_id,
                lamport_ts: self.lamport_ts,
                len: self.len - at_offset,
                is_visible: self.is_visible,
            };

            self.len = at_offset;

            id_registry.split_insertion(self.edit_id, at_offset, split_run_id);

            (new_run, Some(split_run))
        }
    }

    /// TODO: docs
    #[inline]
    pub fn insertion_id(&self) -> InsertionAnchor {
        self.insertion_id
    }

    #[inline]
    pub(super) fn len(&self) -> usize {
        self.len
    }

    /// TODO: docs
    pub fn origin(
        edit_id: InsertionId,
        lamport_ts: LamportTimestamp,
        len: usize,
    ) -> Self {
        debug_assert_eq!(0, edit_id.local_timestamp_at_creation.as_u64());
        debug_assert_eq!(0, lamport_ts.as_u64());

        Self {
            edit_id,
            insertion_id: InsertionAnchor::origin(),
            run_id: RunId::from([u16::MAX / 2]),
            next_run_id: RunId::from([u16::MAX]),
            lamport_ts,
            len,
            is_visible: true,
        }
    }

    /// TODO: docs
    #[inline]
    pub fn run_id(&self) -> &RunId {
        &self.run_id
    }

    /// TODO: docs
    #[inline]
    pub fn split(&mut self, byte_offset: usize) -> Option<Self> {
        if byte_offset < self.len {
            let mut rest = self.clone();
            rest.len = self.len - byte_offset;
            self.len = byte_offset;
            Some(rest)
        } else {
            None
        }
    }
}

/// TODO: docs
#[derive(Copy, Clone)]
pub struct InsertionId {
    /// TODO: docs
    created_by: ReplicaId,

    /// TODO: docs
    local_timestamp_at_creation: LocalTimestamp,
}

impl core::fmt::Debug for InsertionId {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(
            f,
            "{:x}.{}",
            self.created_by.as_u32(),
            self.local_timestamp_at_creation.as_u64()
        )
    }
}

impl InsertionId {
    #[inline]
    pub fn local_ts(&self) -> LocalTimestamp {
        self.local_timestamp_at_creation
    }

    #[inline]
    pub fn new(replica_id: ReplicaId, timestamp: LocalTimestamp) -> Self {
        Self { created_by: replica_id, local_timestamp_at_creation: timestamp }
    }

    #[inline]
    pub fn replica_id(&self) -> ReplicaId {
        self.created_by
    }
}

/// TODO: docs
#[derive(Copy, Clone)]
pub struct InsertionAnchor {
    /// TODO: docs
    inside_of: InsertionId,

    /// TODO: docs
    at_offset: usize,
}

impl core::fmt::Debug for InsertionAnchor {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "{:?} @ {}", self.inside_of, self.at_offset)
    }
}

impl InsertionAnchor {
    /// TODO: docs
    pub fn insertion_id(&self) -> InsertionId {
        self.inside_of
    }

    /// TODO: docs
    pub fn offset(&self) -> usize {
        self.at_offset
    }

    /// TODO: docs
    pub fn origin() -> Self {
        Self {
            inside_of: InsertionId {
                created_by: ReplicaId::zero(),
                local_timestamp_at_creation: LocalTimestamp::default(),
            },
            at_offset: 0,
        }
    }
}

/// TODO: docs
///
/// The `Ord` implementation for `Vec`s [is already][lexi] a lexicographic
/// sort, so we can just derive those traits.
///
/// [lexi]: https://doc.rust-lang.org/std/vec/struct.Vec.html#impl-Ord-for-Vec<,+A>
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RunId {
    /// TODO: docs
    letters: Rc<[u16]>,
}

/// SAFETY: `RunId`s are never shared between different threads.
unsafe impl Send for RunId {}

/// SAFETY: same as above.
unsafe impl Sync for RunId {}

impl core::fmt::Debug for RunId {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        core::fmt::Debug::fmt(&self.letters, f)
    }
}

impl Default for RunId {
    #[inline]
    fn default() -> Self {
        Self { letters: Rc::from([u16::MAX / 2]) }
    }
}

impl<I: IntoIterator<Item = u16>> From<I> for RunId {
    #[inline]
    fn from(iter: I) -> Self {
        Self { letters: iter.into_iter().collect() }
    }
}

impl RunId {
    /// TODO: docs
    ///
    /// # Panics
    ///
    /// This function assumes the left id is the smaller one, and it'll panic
    /// if the left id is greater than or equal to the right id.
    fn between(left: &Self, right: &Self) -> Self {
        debug_assert!(left < right);

        let mut letters = Vec::new();

        let left_then_zero =
            left.letters.iter().copied().chain(core::iter::repeat(0));

        let right_then_max =
            right.letters.iter().copied().chain(core::iter::repeat(u16::MAX));

        for (left, right) in left_then_zero.zip(right_then_max) {
            let halfway = (right - left) / 2;

            letters.push(left + halfway);

            if halfway != 0 {
                break;
            }
        }

        Self { letters: Rc::from(letters) }
    }

    /// TODO: docs
    fn zero() -> Self {
        Self { letters: Rc::from([0]) }
    }
}

/// TODO: docs
#[derive(Clone, Default, PartialEq)]
pub struct RunSummary {
    /// TODO: docs
    pub(super) len: usize,

    /// TODO: docs
    max_run_id: RunId,
}

impl core::fmt::Debug for RunSummary {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "{{ len: {}, max_run_id: {:?} }}", self.len, self.max_run_id)
    }
}

impl AddAssign<&Self> for RunSummary {
    #[inline]
    fn add_assign(&mut self, other: &Self) {
        self.len += other.len;

        if self.max_run_id < other.max_run_id {
            self.max_run_id = other.max_run_id.clone();
        }
    }
}

impl Add<&Self> for RunSummary {
    type Output = Self;

    #[inline]
    fn add(mut self, rhs: &Self) -> Self {
        self += rhs;
        self
    }
}

impl SubAssign<&Self> for RunSummary {
    #[inline]
    fn sub_assign(&mut self, other: &Self) {
        self.len -= other.len;

        if self.max_run_id > other.max_run_id {
            self.max_run_id = other.max_run_id.clone();
        }
    }
}

impl Sub<&Self> for RunSummary {
    type Output = Self;

    #[inline]
    fn sub(mut self, rhs: &Self) -> Self {
        self -= rhs;
        self
    }
}

impl Summarize for EditRun {
    type Summary = RunSummary;

    #[inline]
    fn summarize(&self) -> Self::Summary {
        RunSummary {
            len: self.len * (self.is_visible as usize),
            max_run_id: self.run_id.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_id_0() {
        let left = RunId::from([1]);
        let right = RunId::from([3]);
        assert_eq!(RunId::between(&left, &right), RunId::from([2]));
    }
}
