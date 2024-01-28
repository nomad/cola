mod common;

#[cfg(feature = "serde")]
mod serde {
    use serde::de::DeserializeOwned;
    use serde::ser::Serialize;
    use traces::{ConcurrentTraceInfos, Crdt, Edit, SequentialTrace};

    use super::common;

    trait Encoder {
        const NAME: &'static str;
        fn name() -> impl std::fmt::Display {
            Self::NAME
        }
        fn encode<T: Serialize>(value: &T) -> Vec<u8>;
        fn decode<T: DeserializeOwned>(buf: Vec<u8>) -> T;
    }

    struct SerdeJson;

    impl Encoder for SerdeJson {
        const NAME: &'static str = "serde_json";

        fn encode<T: Serialize>(value: &T) -> Vec<u8> {
            serde_json::to_vec(value).unwrap()
        }

        fn decode<T: DeserializeOwned>(buf: Vec<u8>) -> T {
            serde_json::from_slice(&buf).unwrap()
        }
    }

    struct Bincode;

    impl Encoder for Bincode {
        const NAME: &'static str = "bincode";

        fn encode<T: Serialize>(value: &T) -> Vec<u8> {
            bincode::serialize(value).unwrap()
        }

        fn decode<T: DeserializeOwned>(buf: Vec<u8>) -> T {
            bincode::deserialize(&buf).unwrap()
        }
    }

    struct Zstd<E>(std::marker::PhantomData<E>);

    impl<E: Encoder> Encoder for Zstd<E> {
        const NAME: &'static str = "zstd'd ";

        fn name() -> impl std::fmt::Display {
            format!("{}{}", Self::NAME, E::name())
        }

        fn encode<T: Serialize>(value: &T) -> Vec<u8> {
            zstd::stream::encode_all(&*E::encode(value), 0).unwrap()
        }

        fn decode<T: DeserializeOwned>(buf: Vec<u8>) -> T {
            E::decode(zstd::stream::decode_all(&*buf).unwrap())
        }
    }

    fn test_trace<const N: usize, E: Encoder>(
        trace: ConcurrentTraceInfos<N, common::Replica>,
    ) {
        let ConcurrentTraceInfos { trace, mut peers, final_content, .. } =
            trace;

        for edit in trace.edits() {
            match edit {
                Edit::Insertion(idx, offset, text) => {
                    peers[*idx].local_insert(*offset, text);
                    peers[*idx].assert_invariants();
                },
                Edit::Deletion(idx, start, end) => {
                    peers[*idx].local_delete(*start, *end);
                    peers[*idx].assert_invariants();
                },
                Edit::Merge(idx, edit) => {
                    let encoded = E::encode(edit);
                    let decoded = E::decode(encoded);
                    peers[*idx].remote_merge(&decoded);
                    peers[*idx].assert_invariants();
                },
            }
        }

        for replica in &mut peers {
            replica.merge_backlogged();
        }

        for replica in &peers {
            assert_eq!(replica.buffer, final_content);
        }
    }

    /// Tests that the `friends-forever` trace converges if we serialize and
    /// deserialize every edit before applying it.
    #[test]
    fn serde_friends_forever_round_trip() {
        test_trace::<2, SerdeJson>(traces::friends_forever());
    }

    /// Runs a trace and prints the total size of the serialized `Insertion`s
    /// and `Deletion`s.
    fn serde_sizes<E: Encoder>(trace: &SequentialTrace) {
        let trace = trace.chars_to_bytes();

        let mut replica = cola::Replica::new(1, trace.start_content().len());

        let mut insertions = Vec::new();

        let mut deletions = Vec::new();

        for (start, end, text) in trace.edits() {
            if end > start {
                let deletion = replica.deleted(start..end);
                deletions.push(E::encode(&deletion));
            }

            if !text.is_empty() {
                let insertion = replica.inserted(start, text.len());
                insertions.push(E::encode(&insertion));
            }
        }

        let printed_size = |num_bytes: usize| {
            let num_bytes = num_bytes as f64;

            if num_bytes < 1024.0 {
                format!("{} B", num_bytes)
            } else if num_bytes < 1024.0 * 1024.0 {
                format!("{:.2} KB", num_bytes / 1024.0)
            } else if num_bytes < 1024.0 * 1024.0 * 1024.0 {
                format!("{:.2} MB", num_bytes / 1024.0 / 1024.0)
            } else {
                format!("{:.2} GB", num_bytes / 1024.0 / 1024.0 / 1024.0)
            }
        };

        let replica_size = E::encode(&replica.encode()).len();

        println!("{} | Replica: {}", E::name(), printed_size(replica_size));

        let total_insertions_size =
            insertions.iter().map(Vec::len).sum::<usize>();

        println!(
            "{} | Total insertions: {}",
            E::name(),
            printed_size(total_insertions_size)
        );

        let total_deletions_size =
            deletions.iter().map(Vec::len).sum::<usize>();

        println!(
            "{} | Total deletions: {}",
            E::name(),
            printed_size(total_deletions_size)
        );
    }

    // `cargo t --release --features=serde serde_automerge_json_sizes -- --nocapture`
    #[test]
    fn serde_automerge_json_sizes() {
        serde_sizes::<SerdeJson>(&traces::automerge());
    }

    // `cargo t --release --features=serde serde_automerge_bincode_sizes -- --nocapture`
    #[test]
    fn serde_automerge_bincode_sizes() {
        serde_sizes::<Bincode>(&traces::automerge());
    }

    // `cargo t --release --features=serde serde_automerge_compressed_json_sizes -- --nocapture`
    #[test]
    fn serde_automerge_compressed_json_sizes() {
        serde_sizes::<Zstd<SerdeJson>>(&traces::automerge());
    }

    // `cargo t --release --features=serde serde_automerge_compressed_bincode_sizes -- --nocapture`
    #[test]
    fn serde_automerge_compressed_bincode_sizes() {
        serde_sizes::<Zstd<Bincode>>(&traces::automerge());
    }
}
