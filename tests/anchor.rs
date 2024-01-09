use cola::{AnchorBias, Replica};

/// Tests that an anchor set at the start of the document with left bias always
/// resolves to the start of the document.
#[test]
fn anchor_start_left_bias() {
    let mut replica = Replica::new(1, 0);

    let _ = replica.inserted(0, 5);

    let anchor = replica.create_anchor(0, AnchorBias::Left);

    let _ = replica.inserted(0, 5);
    let _ = replica.inserted(0, 5);

    assert_eq!(replica.resolve_anchor(anchor).unwrap(), 0);
}

/// Tests that an anchor set at the start of the document with right bias
/// resolves to the start of the first visible run at the time of creation.
#[test]
fn anchor_start_right_bias() {
    let mut replica = Replica::new(1, 0);

    let _ = replica.inserted(0, 5);

    let anchor = replica.create_anchor(0, AnchorBias::Right);

    let _ = replica.inserted(0, 5);
    let _ = replica.inserted(0, 5);

    assert_eq!(replica.resolve_anchor(anchor).unwrap(), 10);

    let _ = replica.deleted(0..3);

    assert_eq!(replica.resolve_anchor(anchor).unwrap(), 7);
}

/// Tests that deleted runs are skipped when anchoring to the start of the
/// document with right bias.
#[test]
fn anchor_start_right_bias_skip_deleted() {
    let mut replica = Replica::new(1, 0);

    let _ = replica.inserted(0, 5);

    let _ = replica.deleted(0..2);

    let anchor = replica.create_anchor(0, AnchorBias::Right);

    let _ = replica.inserted(0, 5);

    assert_eq!(replica.resolve_anchor(anchor).unwrap(), 5);
}

/// Tests that an anchor set at the end of the document with left bias resolves
/// to the end of the last visible run at the time of creation.
#[test]
fn anchor_end_left_bias() {
    let mut replica = Replica::new(1, 0);

    let _ = replica.inserted(0, 5);

    let anchor = replica.create_anchor(5, AnchorBias::Left);

    let _ = replica.inserted(5, 5);
    let _ = replica.inserted(10, 5);

    assert_eq!(replica.resolve_anchor(anchor).unwrap(), 5);
}

/// Tests that an anchor set at the en of the document with right bias always
/// resolves to the end of the document.
#[test]
fn anchor_end_right_bias() {
    let mut replica = Replica::new(1, 0);

    let _ = replica.inserted(0, 5);

    let anchor = replica.create_anchor(5, AnchorBias::Right);

    let _ = replica.inserted(5, 5);
    let _ = replica.inserted(10, 5);

    assert_eq!(replica.resolve_anchor(anchor).unwrap(), 15);
}

/// Tests that we stop before the deleted runs when anchoring to the end of the
/// document with left bias.
#[test]
fn anchor_end_left_bias_stop_before_deleted() {
    let mut replica = Replica::new(1, 0);

    let _ = replica.inserted(0, 5);
    let _ = replica.inserted(0, 5);
    let _ = replica.inserted(0, 5);
    let _ = replica.deleted(10..15);
    let _ = replica.deleted(5..10);

    let anchor = replica.create_anchor(5, AnchorBias::Left);

    let _ = replica.inserted(5, 5);
    let _ = replica.inserted(10, 5);

    assert_eq!(replica.resolve_anchor(anchor).unwrap(), 5);
}

/// Tests that the offset of the anchor in the run containing it is not added
/// to the total if the run is deleted.
#[test]
fn anchor_inside_deleted_run() {
    let mut replica = Replica::new(1, 0);

    let _ = replica.inserted(0, 5);
    let _ = replica.inserted(0, 5);

    let anchor = replica.create_anchor(8, AnchorBias::Left);

    let _ = replica.deleted(6..9);

    assert_eq!(replica.resolve_anchor(anchor).unwrap(), 6);
}

/// Tests that an anchor set at the border between two runs with left bias
/// resolves to the end of the run on the left.
#[test]
fn anchor_at_run_border_left_bias() {
    let mut replica = Replica::new(1, 0);

    let _ = replica.inserted(0, 5);
    let _ = replica.inserted(0, 5);

    let anchor = replica.create_anchor(5, AnchorBias::Left);

    let _ = replica.inserted(5, 5);

    assert_eq!(replica.resolve_anchor(anchor).unwrap(), 5);
}

/// Tests that an anchor set at the border between two runs with right bias
/// resolves to the start of the run on the right.
#[test]
fn anchor_at_run_border_right_bias() {
    let mut replica = Replica::new(1, 0);

    let _ = replica.inserted(0, 5);
    let _ = replica.inserted(0, 5);

    let anchor = replica.create_anchor(5, AnchorBias::Right);

    let _ = replica.inserted(5, 5);

    assert_eq!(replica.resolve_anchor(anchor).unwrap(), 10);
}

/// Tests that an anchor set inside a run with left bias resolves to the end of
/// the left fragment if the run is split.
#[test]
fn anchor_in_split_run_left_bias() {
    let mut replica = Replica::new(1, 0);

    let _ = replica.inserted(0, 10);

    let anchor = replica.create_anchor(5, AnchorBias::Left);

    let _ = replica.inserted(5, 5);

    assert_eq!(replica.resolve_anchor(anchor).unwrap(), 5);
}

/// Tests that an anchor set inside a run with right bias resolves to the start
/// of the right fragment if the run is split.
#[test]
fn anchor_in_split_run_right_bias() {
    let mut replica = Replica::new(1, 0);

    let _ = replica.inserted(0, 10);

    let anchor = replica.create_anchor(5, AnchorBias::Right);

    let _ = replica.inserted(5, 5);

    assert_eq!(replica.resolve_anchor(anchor).unwrap(), 10);
}
