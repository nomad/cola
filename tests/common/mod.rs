use std::fmt::Debug;
use std::ops::Range;

use cola::{CrdtEdit, Length, ReplicaId, TextEdit};

pub struct Replica {
    pub buffer: String,
    pub crdt: cola::Replica,
}

impl Debug for Replica {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Replica")
            .field("buffer", &self.buffer)
            .field("crdt", &self.crdt.debug())
            .finish()
    }
}

impl PartialEq<Replica> for Replica {
    fn eq(&self, rhs: &Replica) -> bool {
        self.buffer == rhs.buffer
    }
}

impl PartialEq<&str> for Replica {
    fn eq(&self, rhs: &&str) -> bool {
        self.buffer == *rhs
    }
}

impl PartialEq<Replica> for &str {
    fn eq(&self, rhs: &Replica) -> bool {
        rhs.buffer == *self
    }
}

type Edit = (String, CrdtEdit);

impl Replica {
    pub fn as_btree(&self) -> DebugAsBtree<'_> {
        DebugAsBtree(self)
    }

    pub fn delete(&mut self, byte_range: Range<usize>) -> Edit {
        self.buffer.replace_range(byte_range.clone(), "");

        let edit = self
            .crdt
            .deleted(byte_range.start as Length..byte_range.end as Length);

        (String::new(), edit)
    }

    pub fn fork(&self, id: impl Into<ReplicaId>) -> Self {
        Self { buffer: self.buffer.clone(), crdt: self.crdt.fork(id) }
    }

    pub fn insert<T: Into<String>>(
        &mut self,
        byte_offset: usize,
        text: T,
    ) -> Edit {
        let text = text.into();
        self.buffer.insert_str(byte_offset, text.as_str());
        let edit =
            self.crdt.inserted(byte_offset as Length, text.len() as Length);
        (text, edit)
    }

    pub fn merge(&mut self, (string, edit): &Edit) {
        if let Some(edit) = self.crdt.merge(edit) {
            match edit {
                TextEdit::Insertion(offset, _) => {
                    self.buffer.insert_str(offset, string)
                },

                TextEdit::ContiguousDeletion(range) => {
                    self.buffer.replace_range(range, "")
                },

                TextEdit::SplitDeletion(ranges) => {
                    for range in ranges.into_iter().rev() {
                        self.buffer.replace_range(range, "")
                    }
                },
            }
        }
    }

    pub fn new<T: Into<String>>(id: impl Into<ReplicaId>, text: T) -> Self {
        let buffer = text.into();
        let crdt = cola::Replica::new(id, buffer.len() as Length);
        Self { buffer, crdt }
    }
}

impl traces::Crdt for Replica {
    type EDIT = Edit;

    fn from_str(s: &str) -> Self {
        Self::new(rand::random::<u64>(), s)
    }

    fn fork(&self) -> Self {
        self.fork(rand::random::<u64>())
    }

    fn local_insert(&mut self, offset: usize, text: &str) -> Self::EDIT {
        self.insert(offset, text)
    }

    fn local_delete(&mut self, start: usize, end: usize) -> Self::EDIT {
        self.delete(start..end)
    }

    fn merge(&mut self, remote_edit: &Self::EDIT) {
        self.merge(remote_edit)
    }
}

pub struct DebugAsBtree<'a>(&'a Replica);

impl Debug for DebugAsBtree<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let replica = self.0;

        f.debug_struct("Replica")
            .field("buffer", &replica.buffer)
            .field("crdt", &replica.crdt.debug_as_btree())
            .finish()
    }
}

#[macro_export]
macro_rules! assert_convergence {
    ($one:expr, $two:expr) => {{
        assert_eq!($one, $two);
    }};

    ($one:expr, $two:expr, $three:expr) => {{
        assert_eq!($one, $two);
        assert_eq!($two, $three);
    }};

    ($one:expr, $two:expr, $three:expr, $four:expr) => {{
        assert_eq!($one, $two);
        assert_eq!($two, $three);
        assert_eq!($three, $four);
    }};
}
