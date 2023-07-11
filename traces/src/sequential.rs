use std::io::Read;

pub use crdt_testdata::{TestData, TestPatch};
use flate2::bufread::GzDecoder;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct SequentialTrace(TestData);

impl SequentialTrace {
    pub fn chars_to_bytes(&self) -> Self {
        Self(self.0.chars_to_bytes())
    }

    pub(crate) fn decode_from_gzipped_json(bytes: &[u8]) -> Self {
        let mut decoder = GzDecoder::new(bytes);
        let mut json = Vec::new();
        decoder.read_to_end(&mut json).unwrap();
        let this = serde_json::from_reader(json.as_slice()).unwrap();
        Self(this)
    }

    pub fn edits(&self) -> impl Iterator<Item = (usize, usize, &str)> {
        self.0.txns.iter().flat_map(|txn| {
            txn.patches.iter().map(move |TestPatch(pos, del, text)| {
                (*pos, pos + del, text.as_str())
            })
        })
    }

    pub fn end_content(&self) -> &str {
        self.0.end_content.as_str()
    }

    pub fn num_edits(&self) -> usize {
        self.0.txns.iter().flat_map(|txn| txn.patches.iter()).count()
    }

    pub fn start_content(&self) -> &str {
        self.0.start_content.as_str()
    }
}

pub use traces::*;

mod traces {
    use super::*;

    static AUTOMERGE: &[u8] =
        include_bytes!("../sequential/automerge-paper.json.gz");

    static RUSTCODE: &[u8] = include_bytes!("../sequential/rustcode.json.gz");

    static SEPH_BLOG: &[u8] =
        include_bytes!("../sequential/seph-blog1.json.gz");

    static SVELTECOMPONENT: &[u8] =
        include_bytes!("../sequential/sveltecomponent.json.gz");

    pub fn automerge() -> SequentialTrace {
        SequentialTrace::decode_from_gzipped_json(AUTOMERGE)
    }

    pub fn rustcode() -> SequentialTrace {
        SequentialTrace::decode_from_gzipped_json(RUSTCODE)
    }

    pub fn seph_blog() -> SequentialTrace {
        SequentialTrace::decode_from_gzipped_json(SEPH_BLOG)
    }

    pub fn sveltecomponent() -> SequentialTrace {
        SequentialTrace::decode_from_gzipped_json(SVELTECOMPONENT)
    }
}
