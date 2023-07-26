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
            .field("crdt", &self.crdt.debug_as_btree())
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
    pub fn assert_invariants(&self) {
        self.crdt.assert_invariants();
        assert_eq!(self.buffer.len(), self.crdt.len());
    }

    fn char_to_byte(&self, char_offset: usize) -> usize {
        self.buffer.chars().take(char_offset).map(char::len_utf8).sum()
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
        let edit = self.crdt.deleted(byte_range);
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
        let edit = self.crdt.inserted(byte_offset, text.len());
        (text, edit)
    }

    pub fn len(&self) -> usize {
        self.buffer.len()
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
        let crdt = cola::Replica::new(id, buffer.len());
        let history = HashMap::new();
        Self { buffer, crdt, history }
    }

    pub fn new_with_len(
        id: impl Into<ReplicaId>,
        max_len: usize,
        rng: &mut impl Rng,
    ) -> Self {
        let string =
            (0..max_len).map(|_| rng.gen_range('a'..='z')).collect::<String>();
        Self::new(id, string)
    }

    pub fn random_insert(
        &self,
        rng: &mut impl rand::Rng,
        max_len: usize,
    ) -> (usize, String) {
        assert!(max_len > 0);
        let offset = rng.gen_range(0..=self.buffer.len());
        let text_len = rng.gen_range(1..=max_len);
        let letter = rng.gen_range('a'..='z');
        let text = (0..text_len).map(|_| letter).collect::<String>();
        (offset, text)
    }

    pub fn random_delete(
        &self,
        rng: &mut impl rand::Rng,
        max_len: usize,
    ) -> Range<usize> {
        assert!(!self.buffer.is_empty());

        let start = rng.gen_range(0..self.buffer.len());

        let len = rng.gen_range(1..=max_len);

        let end = if start + len > self.buffer.len() {
            self.buffer.len()
        } else {
            start + len
        };

        start..end
    }

    pub fn random_edit(
        &self,
        rng: &mut impl rand::Rng,
        max_len: usize,
    ) -> RandomEdit {
        let create_insertion = rand::random::<bool>();

        if create_insertion {
            let (offset, text) = self.random_insert(rng, max_len);
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

    fn from_str(id: u64, s: &str) -> Self {
        Self::new(id, s)
    }

    fn fork(&self, new_id: u64) -> Self {
        self.fork(new_id)
    }

    fn local_insert(&mut self, offset: usize, text: &str) -> Self::EDIT {
        let offset = self.char_to_byte(offset);
        self.insert(offset, text)
    }

    fn local_delete(&mut self, start: usize, end: usize) -> Self::EDIT {
        let start = self.char_to_byte(start);
        let end = self.char_to_byte(end);
        self.delete(start..end)
    }

    fn remote_merge(&mut self, remote_edit: &Self::EDIT) {
        self.merge(remote_edit)
    }
}

#[macro_export]
macro_rules! assert_convergence {
    ($slice:expr) => {{
        for replica in $slice[1..].iter() {
            if &$slice[0] != replica {
                panic!("left: {:#?}\nright: {:#?}", &$slice[0], replica);
            }
        }
    }};

    ($one:expr, $two:expr) => {{
        if $one != $two {
            panic!("left: {:#?}\nright: {:#?}", $one, $two);
        }
    }};

    ($one:expr, $two:expr, $three:expr) => {{
        assert_eq!($one, $two, "{:#?} vs {:#?}", $one, $two);
        assert_eq!($two, $three, "{:#?} vs {:#?}", $two, $three);
    }};

    ($one:expr, $two:expr, $three:expr, $four:expr) => {{
        assert_eq!($one, $two, "{:#?} vs {:#?}", $one, $two);
        assert_eq!($two, $three, "{:#?} vs {:#?}", $two, $three);
        assert_eq!($three, $four, "{:#?} vs {:#?}", $three, $four);
    }};

    ($one:expr, $two:expr, $three:expr, $four:expr, $five:expr) => {{
        assert_eq!($one, $two, "{:#?} vs {:#?}", $one, $two);
        assert_eq!($two, $three, "{:#?} vs {:#?}", $two, $three);
        assert_eq!($three, $four, "{:#?} vs {:#?}", $three, $four);
        assert_eq!($four, $five, "{:#?} vs {:#?}", $four, $five);
    }};
}
