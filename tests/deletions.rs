mod common;

use cola::ReplicaId;
use common::Replica;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

#[test]
fn join_consecutive_deletions() {
    let mut replica1 = Replica::new(1, "abc");
    let mut replica2 = replica1.fork(2);

    let del_c = replica1.delete(2..3);
    let del_b = replica1.delete(1..2);

    replica2.merge(&del_c);
    replica2.merge(&del_b);

    assert_eq!(replica1.crdt.num_runs(), 2);
    assert_eq!(replica2.crdt.num_runs(), 2);
}

#[test]
fn deletion_start_continued() {
    let mut replica1 = Replica::new(1, "");
    let mut replica2 = replica1.fork(2);

    let ins_ff = replica1.insert(0, "ff");
    let ins_s = replica2.insert(0, "s");

    replica1.merge(&ins_s);
    replica2.merge(&ins_ff);

    let ins_kkk = replica1.insert(2, "kkk");
    let del_fs = replica2.delete(1..3);

    replica1.merge(&del_fs);
    replica2.merge(&ins_kkk);

    assert_convergence!(replica1, replica2, "fkkk");
}

#[test]
fn random_deletions() {
    let seed = rand::random::<u64>();
    println!("seed: {seed}");
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    test_random_deletions(&mut rng, 200_000, 5, 1000, 5, 5);
}

fn test_random_deletions(
    rng: &mut impl Rng,
    initial_len: usize,
    num_replicas: usize,
    num_cycles: usize,
    deletions_per_cycle: usize,
    max_deletion_len: usize,
) {
    assert!(num_replicas > 1);
    assert!(max_deletion_len > 0);
    assert!(deletions_per_cycle > 0);
    assert!(
        num_replicas * deletions_per_cycle * max_deletion_len * num_cycles
            <= initial_len
    );

    let first_replica = Replica::new_with_len(1, initial_len, rng);

    let mut replicas = vec![first_replica];

    for i in 1..num_replicas {
        replicas.push(replicas[0].fork(i as ReplicaId + 1));
    }

    let mut merge_order = (0..deletions_per_cycle).collect::<Vec<_>>();

    for _ in 0..num_cycles {
        let deletions = replicas
            .iter_mut()
            .map(|replica| {
                (0..deletions_per_cycle)
                    .map(|_| {
                        let range =
                            replica.random_delete(rng, max_deletion_len);
                        replica.delete(range)
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();

        for (replica_idx, replica) in replicas.iter_mut().enumerate() {
            let mut remote_order = (0..num_replicas)
                .filter(|&idx| idx != replica_idx)
                .collect::<Vec<_>>();

            remote_order.shuffle(rng);

            for deletions in remote_order.iter().map(|&idx| &deletions[idx]) {
                merge_order.shuffle(rng);

                for deletion in merge_order.iter().map(|&idx| &deletions[idx])
                {
                    replica.merge(deletion);
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
