mod common;

use common::Replica;
use rand::seq::SliceRandom;

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
fn random_edits() {
    let replica1 = Replica::new(1, "");
    let replica2 = replica1.fork(2);
    let replica3 = replica1.fork(3);
    let replica4 = replica1.fork(4);
    let replica5 = replica1.fork(5);

    let mut replicas = vec![replica1, replica2, replica3, replica4, replica5];

    let edits_per_cycle = 5;

    for _ in 0..1_000 {
        let edits = (0..replicas.len())
            .map(|idx| {
                (0..edits_per_cycle)
                    .map(|_| {
                        let replica = &mut replicas[idx];
                        let (offset, text) = replica.random_insert();
                        replica.insert(offset, text)
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();

        let mut merge_order = (0..replicas.len()).collect::<Vec<_>>();

        merge_order.shuffle(&mut rand::thread_rng());

        for replica_idx in merge_order {
            let len = replicas.len();

            let replica = &mut replicas[replica_idx];

            let mut merge_order =
                (0..len).filter(|&idx| idx != replica_idx).collect::<Vec<_>>();

            merge_order.shuffle(&mut rand::thread_rng());

            for edits_idx in merge_order {
                for edit in &edits[edits_idx] {
                    replica.merge(edit);
                }
            }

            replica.merge_backlogged();
        }

        assert_convergence!(
            replicas[0],
            replicas[1],
            replicas[2],
            replicas[3],
            replicas[4]
        );
    }
}
