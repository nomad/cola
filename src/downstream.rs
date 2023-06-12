pub use insertion::insert;

use crate::*;

mod insertion {
    use super::*;

    /// TODO: docs
    pub fn insert(
        run_tree: &mut RunTree,
        id_registry: &mut RunIdRegistry,
        insertion_id: InsertionId,
        lamport_ts: LamportTimestamp,
        at_anchor: Anchor,
        run_id: RunId,
        text_len: Length,
    ) -> Length {
        debug_assert!(run_id <= run_tree.summary().max_run_id);

        let mut offset = 0;

        if let Some(root_split) = recurse(
            run_tree.root_mut(),
            id_registry,
            insertion_id,
            lamport_ts,
            at_anchor,
            run_id,
            text_len,
            &mut offset,
            &mut false,
            &mut false,
        ) {
            run_tree.replace_root(|old_root| {
                Node::from_children(vec![old_root, root_split])
            })
        }

        offset
    }

    fn recurse(
        node: &mut RunNode,
        id_registry: &mut RunIdRegistry,
        insertion_id: InsertionId,
        lamport_ts: LamportTimestamp,
        anchor: Anchor,
        run_id: RunId,
        text_len: Length,
        offset: &mut Length,
        looking_for_run_id: &mut bool,
        done: &mut bool,
    ) -> Option<RunNode> {
        // put the stuff of InsertionIds that doesn't change over time in
        // another struct and leak it after you construct it so that it's very
        // cheap to clone it.
        //
        // then make every insertion run hold both its Stuff and the Stuff of
        // the next run.
        //
        // maybe none of this is the best option bc we're increasing memory
        // usage to resolve conflicts, which almost never happen in a real app.
        //
        // maybe we should just have a smarter logic here?
        //
        // things to worry about:
        //
        // a) if we get to the fragment and we're inserting in the middle we're
        // done.
        //
        // b) if not we need to check out the next fragment and check if it has
        // the same anchor. if it hasn't we need to have the RunId of the
        // previous run bc we're inserting at the 0 offset.
        //
        // if it has we need to sort using the lamport
        // timestamp and the peer id. if the lamport timestamp is smaller we
        // need to get to the next insertion, and so on.
        //
        // But I'm thinking, maybe

        match node {
            Node::Internal(inode) => {
                for (idx, child) in inode.children_mut().iter_mut().enumerate()
                {
                    let child_summary = child.summary();

                    if child_summary.max_run_id < run_id {
                        *offset += child_summary.len;
                        continue;
                    } else {
                        let split = recurse(
                            child,
                            id_registry,
                            insertion_id,
                            lamport_ts,
                            anchor,
                            run_id,
                            text_len,
                            offset,
                            looking_for_run_id,
                            done,
                        );

                        return split.and_then(|s| {
                            inode.insert(idx + 1, s).map(Node::Internal)
                        });
                    }
                }

                unreachable!();
            },

            Node::Leaf(run) => {
                if *looking_for_run_id {
                    debug_assert_eq!(run.run_id, run_id);

                    if anchor.offset() < run.len() {
                        let (new_run, split_run) = run.bisect_by_remote_run(
                            insertion_id,
                            anchor.offset(),
                            text_len,
                            lamport_ts,
                            id_registry,
                        );

                        // *offset += new_run.visible_len();

                        *done = true;
                    } else {
                    }

                    todo!();
                } else {
                    todo!();
                }
            },
        }
    }
}
