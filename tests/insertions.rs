mod common;

use cola::ReplicaId;
use common::Replica;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

/// Tests the convergence of the state illustrated in Figure 2 of the WOOT
/// paper.
#[test]
fn woot_figure_2() {
    let mut peer1 = Replica::new(1, "ab");
    let mut peer2 = peer1.fork(2);
    let mut peer3 = peer1.fork(3);

    // Peer 1 inserts '1' between 'a' and 'b'.
    let op1 = peer1.insert(1, '1');

    // Peer 2 inserts '2' between 'a' and 'b'.
    let op2 = peer2.insert(1, '2');

    // Peer 1's edit arrives at Peer 3 and gets merged.
    peer3.merge(&op1);

    assert_eq!(peer3, "a1b");

    // Next, Peer 3 inserts '3' between 'a' and '1'.
    let op3 = peer3.insert(1, '3');

    assert_eq!(peer3, "a31b");

    // Now Peer 2's edit arrives at Peer 3, and the '2' should be inserted
    // after the '1', not before the '3'.
    peer3.merge(&op2);

    // The figure ends here but we also complete Peers 1 and 2.

    peer1.merge(&op2);
    peer1.merge(&op3);

    peer2.merge(&op3);
    peer2.merge(&op1);

    assert_convergence!(peer1, peer2, peer3, "a312b");
}

#[test]
fn conflicting_insertions() {
    let peer1 = Replica::new(1, "aa");
    let mut peer2 = peer1.fork(2);
    let mut peer3 = peer1.fork(3);

    let bb = peer2.insert(2, "bb");
    let cc = peer2.insert(2, "cc");
    let dd = peer2.insert(6, "dd");
    let ee = peer2.insert(4, "ee");

    let ff = peer3.insert(2, "ff");

    peer2.merge(&ff);

    peer3.merge(&bb);
    peer3.merge(&cc);
    peer3.merge(&dd);
    peer3.merge(&ee);

    assert_convergence!(peer2, peer3);
}

#[test]
fn conflicting_insertions_2() {
    let mut peer1 = Replica::new(1, "");
    let mut peer2 = peer1.fork(2);

    let sss = peer1.insert(0, "sss");
    let d = peer1.insert(2, "d");

    let m = peer2.insert(0, "m");
    let xx = peer2.insert(0, "xx");

    peer1.merge(&m);
    peer1.merge(&xx);

    peer2.merge(&sss);
    peer2.merge(&d);

    assert_convergence!(peer1, peer2, "xxssdsm");
}

#[test]
fn random_insertions() {
    let seed = rand::random::<u64>();
    println!("seed: {}", seed);
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    test_random_insertions(&mut rng, 5, 1000, 5, 5);
}

fn test_random_insertions(
    rng: &mut impl Rng,
    num_replicas: usize,
    num_cycles: usize,
    insertions_per_cycle: usize,
    max_insertion_len: usize,
) {
    assert!(num_replicas > 1);
    assert!(max_insertion_len > 0);
    assert!(insertions_per_cycle > 0);

    let first_replica = Replica::new(1, "");

    let mut replicas = vec![first_replica];

    for i in 1..num_replicas {
        replicas.push(replicas[0].fork(ReplicaId::from(i as u64 + 1)));
    }

    let mut merge_order = (0..replicas.len()).collect::<Vec<_>>();

    for _ in 0..num_cycles {
        let insertions = replicas
            .iter_mut()
            .map(|replica| {
                (0..insertions_per_cycle)
                    .map(|_| {
                        let (offset, text) =
                            replica.random_insert(rng, max_insertion_len);
                        replica.insert(offset, text)
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();

        merge_order.shuffle(rng);

        for &replica_idx in &merge_order {
            let len = replicas.len();

            let replica = &mut replicas[replica_idx];

            let mut merge_order =
                (0..len).filter(|&idx| idx != replica_idx).collect::<Vec<_>>();

            merge_order.shuffle(rng);

            for idx in merge_order {
                for insertion in &insertions[idx] {
                    replica.merge(insertion);
                }
            }

            replica.merge_backlogged();
        }

        for replica in &replicas {
            replica.assert_invariants();
        }

        assert_convergence!(replicas);
    }
}

#[test]
fn random_edits() {
    let seed = rand::random::<u64>();
    println!("seed: {}", seed);
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    test_random_edits(&mut rng, 5, 1000, 5, 10, 10);
}

fn test_random_edits(
    rng: &mut impl Rng,
    num_replicas: usize,
    num_cycles: usize,
    edits_per_cycle: usize,
    max_insertion_len: usize,
    max_deletion_len: usize,
) {
    let first_replica = Replica::new(1, "");

    let mut replicas = vec![first_replica];

    for i in 1..num_replicas {
        replicas.push(replicas[0].fork(ReplicaId::from(i as u64 + 1)));
    }

    let mut merge_order = (0..replicas.len()).collect::<Vec<_>>();

    for _ in 0..num_cycles {
        let edits = replicas
            .iter_mut()
            .map(|replica| {
                (0..edits_per_cycle)
                    .map(|_| {
                        let edit = replica.random_edit(
                            rng,
                            max_insertion_len,
                            max_deletion_len,
                        );
                        replica.edit(edit)
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();

        merge_order.shuffle(rng);

        for &replica_idx in &merge_order {
            let len = replicas.len();

            let replica = &mut replicas[replica_idx];

            let mut merge_order =
                (0..len).filter(|&idx| idx != replica_idx).collect::<Vec<_>>();

            merge_order.shuffle(rng);

            for idx in merge_order {
                for edit in &edits[idx] {
                    replica.merge(edit);
                }
            }

            replica.merge_backlogged();
        }

        for replica in &replicas {
            replica.assert_invariants();
        }

        assert_convergence!(replicas);
    }
}
