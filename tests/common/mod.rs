use std::collections::HashMap;
use std::fmt::Debug;
use std::ops::Range;

use cola::{CrdtEdit, Length, ReplicaId, TextEdit};
use rand::Rng;

pub struct Replica {
    pub buffer: String,
    pub crdt: cola::Replica,
    history: HashMap<ReplicaId, String>,
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

    pub fn edit(&mut self, edit: RandomEdit) -> Edit {
        match edit {
            RandomEdit::Insertion(byte_offset, text) => {
                self.insert(byte_offset, text)
            },

            RandomEdit::Deletion(byte_range) => self.delete(byte_range),
        }
    }

    pub fn delete(&mut self, byte_range: Range<usize>) -> Edit {
        self.buffer.replace_range(byte_range.clone(), "");

        let edit = self
            .crdt
            .deleted(byte_range.start as Length..byte_range.end as Length);

        (String::new(), edit)
    }

    pub fn fork(&self, id: impl Into<ReplicaId>) -> Self {
        Self {
            buffer: self.buffer.clone(),
            crdt: self.crdt.fork(id),
            history: self.history.clone(),
        }
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
                TextEdit::Insertion(offset, text) => {
                    self.buffer.insert_str(offset, string);

                    self.history
                        .entry(text.inserted_by())
                        .or_insert_with(String::new)
                        .push_str(string);
                },

                TextEdit::ContiguousDeletion(range) => {
                    self.buffer.replace_range(range, "");
                },

                TextEdit::SplitDeletion(ranges) => {
                    for range in ranges.into_iter().rev() {
                        self.buffer.replace_range(range, "");
                    }
                },
            }
        }
    }

    pub fn merge_backlogged(&mut self) {
        for edit in self.crdt.backlogged() {
            match edit {
                TextEdit::Insertion(offset, text) => {
                    let s = &self.history.get(&text.inserted_by()).unwrap()
                        [text.temporal_range()];

                    self.buffer.insert_str(offset, s);
                },

                TextEdit::ContiguousDeletion(range) => {
                    self.buffer.replace_range(range, "");
                },

                TextEdit::SplitDeletion(ranges) => {
                    for range in ranges.into_iter().rev() {
                        self.buffer.replace_range(range, "");
                    }
                },
            }
        }
    }

    pub fn new<T: Into<String>>(id: impl Into<ReplicaId>, text: T) -> Self {
        let buffer = text.into();
        let crdt = cola::Replica::new(id, buffer.len() as Length);
        let history = HashMap::new();
        Self { buffer, crdt, history }
    }

    pub fn random_insert(&self) -> (usize, String) {
        let mut rng = rand::thread_rng();
        let offset = if self.buffer.is_empty() {
            0
        } else {
            rng.gen_range(0..self.buffer.len())
        };
        let text_len = rng.gen_range(1..=5);
        let letter = rng.gen_range('a'..='z');
        let text = (0..text_len).map(|_| letter).collect::<String>();
        (offset, text)
    }

    pub fn random_edit(&self) -> RandomEdit {
        let create_insertion = rand::random::<bool>();

        if create_insertion {
            let (offset, text) = self.random_insert();
            RandomEdit::Insertion(offset, text)
        } else {
            todo!();
        }
    }
}

pub enum RandomEdit {
    Insertion(usize, String),
    Deletion(Range<usize>),
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

    ($one:expr, $two:expr, $three:expr, $four:expr, $five:expr) => {{
        assert_eq!($one, $two);
        assert_eq!($two, $three);
        assert_eq!($three, $four);
        assert_eq!($four, $five);
    }};
}
