use std::io::Read;

use flate2::bufread::GzDecoder;
use serde::Deserialize;

type AgentIdx = usize;

/// An index into the `txns` vector of a `ConcurrentDataSet`.
type TxnIdx = usize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConcurrentDataSet {
    kind: String,
    end_content: String,
    num_agents: usize,
    txns: Vec<Transaction>,
}

impl ConcurrentDataSet {
    pub fn decode_from_gzipped_json(bytes: &[u8]) -> Self {
        let mut decoder = GzDecoder::new(bytes);
        let mut json = Vec::new();
        decoder.read_to_end(&mut json).unwrap();
        let this: Self = serde_json::from_reader(json.as_slice()).unwrap();
        assert_eq!(this.kind, "concurrent");
        this
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Transaction {
    parents: Vec<TxnIdx>,
    agent: AgentIdx,
    patches: Vec<Patch>,
}

/// (position, deleted, text)
#[derive(Deserialize)]
struct Patch(usize, usize, String);

pub trait Crdt: Sized {
    type EDIT: Clone;

    fn from_str(s: &str) -> Self;
    fn fork(&self) -> Self;
    fn local_insert(&mut self, offset: usize, text: &str) -> Self::EDIT;
    fn local_delete(&mut self, start: usize, end: usize) -> Self::EDIT;
    fn remote_merge(&mut self, remote_edit: &Self::EDIT);
}

pub struct ConcurrentTraceInfos<const NUM_PEERS: usize, C: Crdt> {
    pub trace: ConcurrentTrace<NUM_PEERS, C>,
    pub peers: Vec<C>,
    pub final_content: String,
}

impl<const NUM_PEERS: usize, C: Crdt> ConcurrentTraceInfos<NUM_PEERS, C> {
    pub(crate) fn from_data_set(data: ConcurrentDataSet) -> Self {
        let ConcurrentDataSet { end_content, num_agents, txns, .. } = data;
        assert_eq!(num_agents, NUM_PEERS);
        let (trace, peers) = ConcurrentTrace::from_txns(txns);
        Self { trace, peers, final_content: end_content }
    }
}

pub struct ConcurrentTrace<const NUM_PEERS: usize, C: Crdt> {
    edits: Vec<Edit<C>>,
}

pub enum Edit<C: Crdt> {
    Insertion(AgentIdx, usize, String),
    Deletion(AgentIdx, usize, usize),
    Merge(AgentIdx, C::EDIT),
}

impl<const NUM_PEERS: usize, C: Crdt> ConcurrentTrace<NUM_PEERS, C> {
    pub fn edits(&self) -> impl Iterator<Item = &Edit<C>> {
        self.edits.iter()
    }

    fn from_txns(txns: Vec<Transaction>) -> (Self, Vec<C>) {
        let mut edits = Vec::new();

        let mut batches: Vec<Vec<C::EDIT>> = Vec::new();

        let mut peers = Self::init_peers("");

        for txn in txns {
            let peer = &mut peers[txn.agent];

            for idx in txn.parents {
                for edit in &batches[idx] {
                    peer.remote_merge(edit);
                    edits.push(Edit::Merge(txn.agent, edit.clone()));
                }
            }

            let mut batch = Vec::new();

            for &Patch(pos, del, ref text) in &txn.patches {
                if del > 0 {
                    batch.push(peer.local_delete(pos, pos + del));
                    edits.push(Edit::Deletion(txn.agent, pos, pos + del));
                }
                if !text.is_empty() {
                    batch.push(peer.local_insert(pos, text));
                    edits.push(Edit::Insertion(txn.agent, pos, text.clone()));
                }
            }

            batches.push(batch);
        }

        // Here we regenerate the peers vector, which in general will produce a
        // different vector of peers from the one we used to generate the
        // edits.
        //
        // In theory this doesn't matter because the concurrent data sets are
        // guaranteed not to contain any concurrent insertions.

        (Self { edits }, Self::init_peers(""))
    }

    fn init_peers(starting_text: &str) -> Vec<C> {
        let mut peers = Vec::with_capacity(NUM_PEERS);

        let first_peer = C::from_str(starting_text);

        peers.push(first_peer);

        for _ in 1..NUM_PEERS {
            let first_peer = &peers[0];
            peers.push(first_peer.fork());
        }

        assert_eq!(peers.len(), NUM_PEERS);

        peers
    }

    pub fn num_edits(&self) -> usize {
        self.edits.len()
    }
}

pub use traces::*;

mod traces {
    use super::*;

    static FRIENDS_FOREVER: &[u8] =
        include_bytes!("../concurrent/friendsforever.json.gz");

    pub fn friends_forever<C: Crdt>() -> ConcurrentTraceInfos<2, C> {
        let set = ConcurrentDataSet::decode_from_gzipped_json(FRIENDS_FOREVER);
        ConcurrentTraceInfos::from_data_set(set)
    }
}
