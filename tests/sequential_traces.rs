mod common;

use common::Replica;
use traces::SequentialTrace;

fn test_trace(trace: &SequentialTrace) {
    let trace = trace.chars_to_bytes();

    let mut replica = Replica::new(1, trace.start_content());

    for i in 0..1 {
        for (start, end, text) in trace.edits() {
            if end > start {
                replica.delete(start..end);
                replica.assert_invariants();
            }

            if !text.is_empty() {
                replica.insert(start, text);
                replica.assert_invariants();
            }
        }

        if i == 0 {
            assert_eq!(replica, trace.end_content());
        } else {
            assert_eq!(replica.len(), (trace.end_content().len() * (i + 1)));
        }
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
