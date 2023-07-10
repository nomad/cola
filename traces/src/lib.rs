mod concurrent;

use std::io::Read;

use concurrent::ConcurrentDataSet;
pub use concurrent::{
    ConcurrentTraceInfos,
    Crdt,
    Edit,
    ReplayableConcurrentTrace,
};
pub use crdt_testdata::{TestData, TestPatch};
use flate2::bufread::GzDecoder;

static AUTOMERGE: &[u8] =
    include_bytes!("../sequential/automerge-paper.json.gz");

static RUSTCODE: &[u8] = include_bytes!("../sequential/rustcode.json.gz");

static SEPH_BLOG: &[u8] = include_bytes!("../sequential/seph-blog1.json.gz");

static SVELTECOMPONENT: &[u8] =
    include_bytes!("../sequential/sveltecomponent.json.gz");

static FRIENDS_FOREVER: &[u8] =
    include_bytes!("../concurrent/friendsforever.json.gz");

fn decode_sequential(bytes: &[u8]) -> TestData {
    let mut decoder = GzDecoder::new(bytes);
    let mut json = Vec::new();
    decoder.read_to_end(&mut json).unwrap();
    serde_json::from_reader(json.as_slice()).unwrap()
}

pub fn automerge() -> TestData {
    decode_sequential(AUTOMERGE)
}

pub fn rustcode() -> TestData {
    decode_sequential(RUSTCODE)
}

pub fn seph_blog() -> TestData {
    decode_sequential(SEPH_BLOG)
}

pub fn sveltecomponent() -> TestData {
    decode_sequential(SVELTECOMPONENT)
}

pub fn friends_forever<C: Crdt>() -> ConcurrentTraceInfos<2, C> {
    let data = ConcurrentDataSet::decode_from_gzipped_json(FRIENDS_FOREVER);
    ConcurrentTraceInfos::from_data_set(data)
}
