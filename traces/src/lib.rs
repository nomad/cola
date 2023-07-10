use std::io::Read;

pub use crdt_testdata::{TestData, TestPatch};
use flate2::bufread::GzDecoder;

static AUTOMERGE: &[u8] =
    include_bytes!("../sequential/automerge-paper.json.gz");

static RUSTCODE: &[u8] = include_bytes!("../sequential/rustcode.json.gz");

static SEPH_BLOG: &[u8] = include_bytes!("../sequential/seph-blog1.json.gz");

static SVELTECOMPONENT: &[u8] =
    include_bytes!("../sequential/sveltecomponent.json.gz");

fn decode(bytes: &[u8]) -> TestData {
    let mut decoder = GzDecoder::new(bytes);
    let mut json = Vec::new();
    decoder.read_to_end(&mut json).unwrap();
    serde_json::from_reader(json.as_slice()).unwrap()
}

pub fn automerge() -> TestData {
    decode(AUTOMERGE)
}

pub fn rustcode() -> TestData {
    decode(RUSTCODE)
}

pub fn seph_blog() -> TestData {
    decode(SEPH_BLOG)
}

pub fn sveltecomponent() -> TestData {
    decode(SVELTECOMPONENT)
}
