use cola::{Length, Replica};
use traces::{TestData, TestPatch};

fn test_trace(trace: &TestData) {
    let trace = trace.chars_to_bytes();

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
    test_trace(&traces::automerge());
}

#[test]
fn trace_rustcode() {
    test_trace(&traces::rustcode());
}

#[test]
fn trace_seph_blog() {
    test_trace(&traces::seph_blog());
}

#[test]
fn trace_sveltecomponent() {
    test_trace(&traces::sveltecomponent());
}
