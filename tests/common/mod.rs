use std::ops::Range;

use cola::CrdtEdit;

pub struct Replica<B: Buffer> {
    buffer: B,
    crdt: cola::Replica,
}

impl<B: Buffer + Clone> Clone for Replica<B> {
    fn clone(&self) -> Self {
        Self { buffer: self.buffer.clone(), crdt: self.crdt.clone() }
    }
}

impl<B: Buffer + std::fmt::Debug> std::fmt::Debug for Replica<B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Replica")
            .field("buffer", &self.buffer)
            .field("crdt", &self.crdt.debug())
            .finish()
    }
}

impl<B: Buffer + for<'a> PartialEq<&'a str>> PartialEq<&str> for Replica<B> {
    fn eq(&self, rhs: &&str) -> bool {
        self.buffer == rhs
    }
}

impl<B: Buffer + for<'a> PartialEq<&'a str>> PartialEq<Replica<B>> for &str {
    fn eq(&self, rhs: &Replica<B>) -> bool {
        rhs.buffer == self
    }
}

impl<B: Buffer> Replica<B> {
    pub fn delete(&mut self, byte_range: Range<usize>) -> CrdtEdit {
        self.buffer.delete(byte_range.clone());
        self.crdt.deleted(byte_range)
    }

    pub fn insert<T: Into<String>>(
        &mut self,
        byte_offset: usize,
        text: T,
    ) -> CrdtEdit {
        let text = text.into();
        self.buffer.insert(byte_offset, text.as_str());
        self.crdt.inserted(byte_offset, text)
    }

    pub fn merge(&mut self, crdt_edit: &CrdtEdit) {
        if let Some(edit) =
            self.crdt.merge(std::borrow::Cow::Borrowed(crdt_edit))
        {
            self.buffer.replace(edit.range, &edit.content);
        }
    }

    pub fn new<T: Into<B>>(text: T) -> Self {
        let buffer = text.into();
        let crdt = cola::Replica::from_chunks(buffer.chunks());
        Self { buffer, crdt }
    }
}

pub trait Buffer {
    type Chunks<'a>: Iterator<Item = &'a str>
    where
        Self: 'a;

    fn chunks(&self) -> Self::Chunks<'_>;

    fn from_chunks<'a, Chunks>(chunks: Chunks) -> Self
    where
        Chunks: Iterator<Item = &'a str>;

    fn insert(&mut self, byte_offset: usize, text: &str);

    fn delete(&mut self, byte_range: Range<usize>);

    fn replace(&mut self, byte_range: Range<usize>, text: &str) {
        let start = byte_range.start;
        self.delete(byte_range);
        self.insert(start, text);
    }
}

impl Buffer for String {
    type Chunks<'a> = std::iter::Once<&'a str>;

    fn chunks(&self) -> Self::Chunks<'_> {
        std::iter::once(self)
    }

    fn from_chunks<'a, Chunks>(chunks: Chunks) -> Self
    where
        Chunks: Iterator<Item = &'a str>,
    {
        chunks.collect()
    }

    fn insert(&mut self, byte_offset: usize, text: &str) {
        self.insert_str(byte_offset, text);
    }

    fn delete(&mut self, byte_range: Range<usize>) {
        self.replace_range(byte_range, "");
    }

    fn replace(&mut self, byte_range: Range<usize>, text: &str) {
        self.replace_range(byte_range, text);
    }
}
