use std::ops::Range;

use cola::{Deletion, Replica, ReplicaId};

struct Document {
    buffer: String,
    crdt: Replica,
}

struct Insertion {
    text: String,
    crdt: cola::Insertion,
}

impl Document {
    fn new<S: Into<String>>(text: S, replica_id: ReplicaId) -> Self {
        let buffer = text.into();
        let crdt = Replica::new(replica_id, buffer.len());
        Document { buffer, crdt }
    }

    fn fork(&self, new_replica_id: ReplicaId) -> Self {
        let crdt = self.crdt.fork(new_replica_id);
        Document { buffer: self.buffer.clone(), crdt }
    }

    fn insert<S: Into<String>>(
        &mut self,
        insert_at: usize,
        text: S,
    ) -> Insertion {
        let text = text.into();
        self.buffer.insert_str(insert_at, &text);
        let insertion = self.crdt.inserted(insert_at, text.len());
        Insertion { text, crdt: insertion }
    }

    fn delete(&mut self, range: Range<usize>) -> Deletion {
        self.buffer.replace_range(range.clone(), "");
        self.crdt.deleted(range)
    }

    fn integrate_insertion(&mut self, insertion: Insertion) {
        if let Some(offset) = self.crdt.integrate_insertion(&insertion.crdt) {
            self.buffer.insert_str(offset, &insertion.text);
        }
    }

    fn integrate_deletion(&mut self, deletion: Deletion) {
        let ranges = self.crdt.integrate_deletion(&deletion);
        for range in ranges.into_iter().rev() {
            self.buffer.replace_range(range, "");
        }
    }
}

fn main() {
    let mut peer_1 = Document::new("Hello, world", 1);
    let mut peer_2 = peer_1.fork(2);

    let delete_comma = peer_1.delete(5..6);
    let insert_exclamation = peer_2.insert(12, "!");

    peer_1.integrate_insertion(insert_exclamation);
    peer_2.integrate_deletion(delete_comma);

    assert_eq!(peer_1.buffer, "Hello world!");
    assert_eq!(peer_2.buffer, "Hello world!");
}
