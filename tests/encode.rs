use cola::{Length, Replica, ReplicaId};

#[cfg(feature = "encode")]
#[test]
fn encode_automerge() {
    let automerge = traces::automerge().chars_to_bytes();

    let mut replica =
        Replica::new(0, automerge.start_content().len() as Length);

    for (start, end, text) in automerge.edits() {
        replica.deleted(start..end);
        replica.inserted(start, text.len());
    }

    let encoded = replica.encode();

    let decoded = Replica::decode(1, &encoded).unwrap();

    assert_eq!(decoded.id(), ReplicaId::from(1));

    assert!(replica.eq_decoded(&decoded));
}
