#[cfg(feature = "encode")]
mod encode {
    use cola::Replica;

    /// Tests an encode-decode round-trip of an empty `Replica`.
    #[test]
    fn encode_empty() {
        let replica = Replica::new(1, 42);
        let encoded = replica.encode();
        let decoded = Replica::decode(2, &encoded).unwrap();
        assert!(replica.eq_decoded(&decoded));
    }

    /// Tests an encode-decode round-trip of a `Replica` that has gone through
    /// the `automerge` trace.
    #[test]
    fn encode_automerge() {
        let automerge = traces::automerge().chars_to_bytes();

        let mut replica = Replica::new(1, automerge.start_content().len());

        for (start, end, text) in automerge.edits() {
            let _ = replica.deleted(start..end);
            let _ = replica.inserted(start, text.len());
        }

        let encoded = replica.encode();

        let decoded = Replica::decode(2, &encoded).unwrap();

        assert!(replica.eq_decoded(&decoded));
    }
}
