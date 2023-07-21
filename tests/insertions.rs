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
    let peer0 = Replica::new(0, "aa");
    let mut peer1 = peer0.fork(1);
    let mut peer2 = peer0.fork(2);

    let one_a = peer1.insert(2, "bb");
    let one_b = peer1.insert(2, "cc");
    let one_c = peer1.insert(6, "dd");
    let one_d = peer1.insert(4, "ee");

    let two_a = peer2.insert(2, "ff");

    peer1.merge(&two_a);

    peer2.merge(&one_a);
    peer2.merge(&one_b);
    peer2.merge(&one_c);
    peer2.merge(&one_d);

    assert_convergence!(peer1, peer2);
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
    test_random_insertions(&mut rng, 5, 1_000, 5, 5);
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

    let first_replica = Replica::new(0, "");

    let mut replicas = vec![first_replica];

    for i in 1..num_replicas {
        replicas.push(replicas[0].fork(ReplicaId::from(i as u64)));
    }

    for _ in 0..num_cycles {
        let edits = (0..replicas.len())
            .map(|idx| {
                (0..insertions_per_cycle)
                    .map(|_| {
                        let replica = &mut replicas[idx];
                        let (offset, text) =
                            replica.random_insert(rng, max_insertion_len);
                        replica.insert(offset, text)
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();

        let mut merge_order = (0..replicas.len()).collect::<Vec<_>>();

        merge_order.shuffle(rng);

        for replica_idx in merge_order {
            let len = replicas.len();

            let replica = &mut replicas[replica_idx];

            let mut merge_order =
                (0..len).filter(|&idx| idx != replica_idx).collect::<Vec<_>>();

            merge_order.shuffle(rng);

            for edits_idx in merge_order {
                for edit in &edits[edits_idx] {
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
