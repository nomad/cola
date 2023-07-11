mod common;

use common::Replica;
use traces::{ConcurrentTraceInfos, Edit};

fn test_trace<const N: usize>(trace: ConcurrentTraceInfos<N, Replica>) {
    let ConcurrentTraceInfos { trace, mut peers, final_content, .. } = trace;

    for edit in trace.edits() {
        match edit {
            Edit::Insertion(idx, offset, text) => {
                peers[*idx].insert(*offset, text);
            },
            Edit::Deletion(idx, start, end) => {
                peers[*idx].delete(*start..*end);
            },
            Edit::Merge(idx, edit) => {
                peers[*idx].merge(edit);
            },
        }
    }

    for replica in &peers {
        assert_eq!(replica.buffer, final_content);
    }
}

#[test]
fn test_friends_forever() {
    test_trace(traces::friends_forever());
}
