use std::fmt::Debug;
use std::ops::Range;

use cola::{CrdtEdit, Length, TextEdit};

#[derive(Clone)]
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

impl Replica {
    pub fn as_btree(&self) -> DebugAsBtree<'_> {
        DebugAsBtree(self)
    }

    pub fn delete(&mut self, byte_range: Range<usize>) -> CrdtEdit {
        self.buffer.replace_range(byte_range.clone(), "");
        self.crdt.deleted(byte_range.start as Length..byte_range.end as Length)
    }

    pub fn insert<T: Into<String>>(
        &mut self,
        byte_offset: usize,
        text: T,
    ) -> CrdtEdit {
        let text = text.into();
        self.buffer.insert_str(byte_offset, text.as_str());
        self.crdt.inserted(byte_offset as Length, text.len() as Length)
    }

    pub fn merge(&mut self, crdt_edit: &CrdtEdit) {
        if let Some(edit) = self.crdt.merge(crdt_edit) {
            match edit {
                _ => todo!(),
            }
            // self.buffer.replace(edit.range, "");
        }
    }

    pub fn new<T: Into<String>>(text: T) -> Self {
        let buffer = text.into();
        let crdt = cola::Replica::new(buffer.len() as Length);
        Self { buffer, crdt }
    }
}

impl traces::Crdt for Replica {
    type EDIT = CrdtEdit;

    fn from_str(s: &str) -> Self {
        Self::new(s)
    }

    fn fork(&self) -> Self {
        self.clone()
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
