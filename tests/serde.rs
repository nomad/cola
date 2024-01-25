mod common;

#[cfg(feature = "serde")]
mod serde {
    use traces::{ConcurrentTraceInfos, Crdt, Edit};

    use super::common::{self, Replica};

    type Encode = fn(&common::Edit) -> Vec<u8>;

    type Decode = fn(Vec<u8>) -> common::Edit;

    fn test_trace<const N: usize>(
        trace: ConcurrentTraceInfos<N, Replica>,
        encode: Encode,
        decode: Decode,
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
                    let encoded = encode(edit);
                    let decoded = decode(encoded);
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
        test_trace(
            traces::friends_forever(),
            serde_json_encode,
            serde_json_decode,
        );
    }

    fn serde_json_encode(edit: &common::Edit) -> Vec<u8> {
        serde_json::to_vec(edit).unwrap()
    }

    fn serde_json_decode(buf: Vec<u8>) -> common::Edit {
        serde_json::from_slice(&buf).unwrap()
    }
}
