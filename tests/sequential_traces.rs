use cola::{Length, Replica};
use traces::SequentialTrace;

fn test_trace(trace: &SequentialTrace) {
    let trace = trace.chars_to_bytes();

    let mut replica = Replica::new(trace.start_content().len() as Length);

    for i in 0..1 {
        for (start, end, text) in trace.edits() {
            let start = start as Length;
            let end = end as Length;

            if end > start {
                replica.deleted(start..end);
                replica.assert_invariants();
            }

            if !text.is_empty() {
                replica.inserted(start, text.len() as Length);
                replica.assert_invariants();
            }
        }

        assert_eq!(
            replica.len(),
            (trace.end_content().len() * (i + 1)) as Length
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
