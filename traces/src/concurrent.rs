use std::collections::HashMap;
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

pub trait Crdt: core::fmt::Debug + Sized {
    type EDIT: Clone + core::fmt::Debug;

    fn from_str(id: u64, s: &str) -> Self;
    fn fork(&self, new_id: u64) -> Self;
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

#[derive(Debug)]
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
        let mut edits_in_txns = Vec::new();

        let mut version_vectors = HashMap::new();

        let mut agents = Self::init_peers("");

        let mut ops = Vec::new();

        for (txn_idx, txn) in txns.iter().enumerate() {
            let agent = &mut agents[txn.agent];

            let version_vector =
                version_vectors.entry(txn.agent).or_insert_with(Vec::new);

            for &parent_idx in &txn.parents {
                recursive_merge(
                    agent,
                    txn.agent,
                    txns.as_slice(),
                    &edits_in_txns,
                    version_vector,
                    &mut ops,
                    parent_idx,
                );
            }

            let mut edits = Vec::new();

            for &Patch(pos, del, ref text) in &txn.patches {
                if del > 0 {
                    edits.push(agent.local_delete(pos, pos + del));
                    ops.push(Edit::Deletion(txn.agent, pos, pos + del));
                }
                if !text.is_empty() {
                    edits.push(agent.local_insert(pos, text));
                    ops.push(Edit::Insertion(txn.agent, pos, text.clone()));
                }
            }

            edits_in_txns.push(edits);

            version_vector.push(txn_idx);
        }

        for (agent_idx, agent) in agents.iter_mut().enumerate() {
            let version_vector =
                version_vectors.entry(agent_idx).or_insert_with(Vec::new);

            for txn_idx in 0..txns.len() {
                recursive_merge(
                    agent,
                    agent_idx,
                    txns.as_slice(),
                    &edits_in_txns,
                    version_vector,
                    &mut ops,
                    txn_idx,
                );
            }
        }

        (Self { edits: ops }, Self::init_peers(""))
    }

    fn init_peers(starting_text: &str) -> Vec<C> {
        let mut peers = Vec::with_capacity(NUM_PEERS);

        let first_peer = C::from_str(1, starting_text);

        peers.push(first_peer);

        for i in 1..NUM_PEERS {
            let first_peer = &peers[0];
            peers.push(first_peer.fork(i as u64 + 1));
        }

        assert_eq!(peers.len(), NUM_PEERS);

        peers
    }

    pub fn num_edits(&self) -> usize {
        self.edits.len()
    }
}

fn recursive_merge<C: Crdt>(
    agent: &mut C,
    agent_idx: AgentIdx,
    txns: &[Transaction],
    edits_in_txns: &[Vec<C::EDIT>],
    version_vector: &mut Vec<TxnIdx>,
    ops: &mut Vec<Edit<C>>,
    txn_idx: TxnIdx,
) {
    if version_vector.contains(&txn_idx) {
        return;
    }

    let txn = &txns[txn_idx];

    for &parent_idx in &txn.parents {
        recursive_merge::<C>(
            agent,
            agent_idx,
            txns,
            edits_in_txns,
            version_vector,
            ops,
            parent_idx,
        );
    }

    let edits = &edits_in_txns[txn_idx];

    for edit in edits {
        agent.remote_merge(edit);
        ops.push(Edit::Merge(agent_idx, edit.clone()));
    }

    version_vector.push(txn_idx);
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
