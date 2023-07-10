use cola::{Length, Replica};
use traces::TestPatch;

fn test_trace(trace: &traces::TestData) {
    let mut replica = Replica::new(trace.start_content.len() as Length);

    for i in 0..1 {
        for txn in trace.txns.iter() {
            for &TestPatch(pos, del, ref ins) in &txn.patches {
                if del > 0 {
                    let start = pos as Length;
                    let end = start + del as Length;
                    replica.deleted(start..end);
                    replica.assert_invariants();
                }

                if !ins.is_empty() {
                    replica.inserted(pos as Length, ins.len() as Length);
                    replica.assert_invariants();
                }
            }
        }

        assert_eq!(
            replica.len(),
            (trace.end_content.len() * (i + 1)) as Length
        );
    }
}

#[test]
fn trace_automerge() {
    test_trace(&traces::automerge().chars_to_bytes());
}

#[test]
fn trace_rustcode() {
    test_trace(&traces::rustcode().chars_to_bytes());
}

#[test]
fn trace_seph_blog() {
    test_trace(&traces::seph_blog().chars_to_bytes());
}

#[test]
fn trace_sveltecomponent() {
    test_trace(&traces::sveltecomponent().chars_to_bytes());
}
