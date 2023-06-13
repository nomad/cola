pub use delete::delete;
pub use insert::insert;

use crate::*;

fn update_summary(
    tot_summary: &mut RunSummary,
    old_summary: RunSummary,
    new_summary: RunSummary,
) {
    tot_summary.len -= old_summary.len;
    tot_summary.len += new_summary.len;

    // max run id has increased
    if old_summary.max_run_id < new_summary.max_run_id {
        if tot_summary.max_run_id < new_summary.max_run_id {
            tot_summary.max_run_id = new_summary.max_run_id;
        }
    }
    // max run id has decreased
    else if old_summary.max_run_id > new_summary.max_run_id {
        if tot_summary.max_run_id == old_summary.max_run_id {
            tot_summary.max_run_id = new_summary.max_run_id;
        }
    }
}

mod insert {
    //! This module handles the logic used to create [`CrdtEdit`]s after the
    //! user locally inserts some text in their buffer.

    use super::*;

    /// TODO: docs
    pub fn insert(
        run_tree: &mut RunTree,
        run_pointers: &mut RunIdRegistry,
        local_clock: &mut LocalClock,
        lamport_clock: &mut LamportClock,
        this_id: ReplicaId,
        at_offset: Length,
        run_len: Length,
    ) -> Anchor {
        let root = match run_tree.root_mut() {
            Node::Internal(inode) => inode,

            Node::Leaf(run) => {
                let (new_run, split_run) = run.bisect_by_local_run(
                    this_id,
                    run_len,
                    at_offset,
                    local_clock,
                    lamport_clock,
                    run_pointers,
                );

                let Some(new_run) = new_run else {
                    return Anchor::new(run.id().clone(), at_offset);
                };

                let anchor = new_run.anchor().clone();

                let new_run = Node::Leaf(new_run);

                run_tree.replace_root(|first_run| {
                    let new_children =
                        if let Some(split_run) = split_run.map(Node::Leaf) {
                            vec![first_run, new_run, split_run]
                        } else {
                            vec![first_run, new_run]
                        };

                    Node::from_children(new_children)
                });

                return anchor;
            },
        };

        let (insertion_id, root_split) = inserted(
            root,
            run_pointers,
            local_clock,
            lamport_clock,
            this_id,
            at_offset,
            run_len,
        );

        if let Some(root_split) = root_split.map(Node::Internal) {
            run_tree.replace_root_with_current_and(root_split);
        }

        insertion_id
    }

    /// TODO: docs
    fn inserted(
        inode: &mut RunInode,
        run_pointers: &mut RunIdRegistry,
        local_clock: &mut LocalClock,
        lamport_clock: &mut LamportClock,
        this_id: ReplicaId,
        at_offset: Length,
        run_len: Length,
    ) -> (Anchor, Option<RunInode>) {
        let mut offset = 0;

        for (idx, child) in inode.children_mut().iter_mut().enumerate() {
            let child_len = child.summary().len();

            offset += child_len;

            if offset >= at_offset {
                offset -= child_len;

                let old_summary = child.summary().into_owned();

                match child {
                    Node::Internal(child) => {
                        let (id, split) = inserted(
                            child,
                            run_pointers,
                            local_clock,
                            lamport_clock,
                            this_id,
                            at_offset - offset,
                            run_len,
                        );

                        let new_summary = child.summary().clone();

                        update_summary(
                            inode.summary_mut(),
                            old_summary,
                            new_summary,
                        );

                        let split = split.and_then(|s| {
                            inode.insert(idx + 1, Node::Internal(s))
                        });

                        return (id, split);
                    },

                    Node::Leaf(run) => {
                        let (new_run, split_run) = run.bisect_by_local_run(
                            this_id,
                            run_len,
                            at_offset - offset,
                            local_clock,
                            lamport_clock,
                            run_pointers,
                        );

                        let insertion_id = run.id().clone();

                        let new_summary = run.summarize();

                        update_summary(
                            inode.summary_mut(),
                            old_summary,
                            new_summary,
                        );

                        let Some(new_run) = new_run else {
                            return (Anchor::new(insertion_id, at_offset - offset), None);
                        };

                        let anchor = new_run.anchor().clone();

                        let new_run = Node::Leaf(new_run);

                        let offset = idx + 1;

                        let split = if let Some(split_run) =
                            split_run.map(Node::Leaf)
                        {
                            inode
                                .insert_two(offset, new_run, offset, split_run)
                        } else {
                            inode.insert(offset, new_run)
                        };

                        return (anchor, split);
                    },
                }
            }
        }

        unreachable!()
    }
}

mod delete {
    //! This module handles the logic used to create [`CrdtEdit`]s after the
    //! user locally deletes some text in their buffer.

    use core::ops::Range;

    use super::*;

    /// TODO: docs
    pub fn delete(
        run_tree: &mut RunTree,
        id_registry: &mut RunIdRegistry,
        range: Range<Length>,
    ) {
        let root = match run_tree.root_mut() {
            Node::Internal(inode) => inode,

            Node::Leaf(edit_run) => {
                let (deleted, rest) =
                    edit_run.delete_range(range, id_registry);

                if let Some(deleted) = deleted.map(Node::Leaf) {
                    run_tree.replace_root(|old_root| {
                        let children = if let Some(rest) = rest.map(Node::Leaf)
                        {
                            vec![old_root, deleted, rest]
                        } else {
                            vec![old_root, deleted]
                        };

                        Node::from_children(children)
                    })
                }

                return;
            },
        };

        if let Some(root_split) =
            deleted(root, id_registry, range).map(Node::Internal)
        {
            run_tree.replace_root_with_current_and(root_split);
        }
    }

    fn deleted(
        inode: &mut RunInode,
        id_registry: &mut RunIdRegistry,
        mut range: Range<Length>,
    ) -> Option<RunInode> {
        let mut offset = 0;

        for (idx, child) in inode.children_mut().iter_mut().enumerate() {
            let child_len = child.summary().len;

            offset += child_len;

            if offset <= range.start {
                continue;
            }

            if offset >= range.end {
                offset -= child_len;

                range.start -= offset;
                range.end -= offset;

                match child {
                    Node::Internal(child) => {
                        let old_summary = child.summary().clone();

                        let split = deleted(child, id_registry, range);

                        let new_summary = child.summary().clone();

                        update_summary(
                            inode.summary_mut(),
                            old_summary,
                            new_summary,
                        );

                        return split.and_then(|s| {
                            inode.insert(idx + 1, Node::Internal(s))
                        });
                    },

                    Node::Leaf(edit_run) => {
                        let old_summary = edit_run.summarize();

                        let (deleted, rest) =
                            edit_run.delete_range(range, id_registry);

                        let new_summary = edit_run.summarize();

                        update_summary(
                            inode.summary_mut(),
                            old_summary,
                            new_summary,
                        );

                        let deleted = deleted.map(Node::Leaf)?;

                        let offset = idx + 1;

                        return if let Some(rest) = rest.map(Node::Leaf) {
                            inode.insert_two(offset, deleted, offset, rest)
                        } else {
                            inode.insert(offset, deleted)
                        };
                    },
                }
            } else {
                return delete_range_in_deepest(inode, id_registry, range);
            }
        }

        unreachable!();
    }

    fn delete_range_in_deepest(
        inode: &mut RunInode,
        id_registry: &mut RunIdRegistry,
        range: Range<Length>,
    ) -> Option<RunInode> {
        let mut start_idx = 0;

        let mut end_idx = 0;

        let mut extra_from_start = None;

        let mut extra_from_end = None;

        let mut children = inode.children_mut().iter_mut().enumerate();

        let mut offset = 0;

        for (idx, child) in children.by_ref() {
            let child_len = child.summary().len;

            offset += child_len;

            if offset > range.start {
                start_idx = idx;
                let delete_from = range.start + child_len - offset;
                extra_from_start =
                    something_start(child, id_registry, delete_from);
                break;
            }
        }

        for (idx, child) in children {
            let child_len = child.summary().len;

            offset += child_len;

            if offset >= range.end {
                end_idx = idx;
                let delete_up_to = range.end + child_len - offset;
                extra_from_end =
                    something_end(child, id_registry, delete_up_to);
                break;
            } else {
                delete_node(child);
            }
        }

        *inode.summary_mut() = inode.summarize();

        match (extra_from_start, extra_from_end) {
            (Some(start), Some(end)) => {
                inode.insert_two(start_idx + 1, start, end_idx + 1, end)
            },

            (Some(start), None) => inode.insert(start_idx + 1, start),

            (None, Some(end)) => inode.insert(end_idx + 1, end),

            (None, None) => None,
        }
    }

    fn something_start(
        node: &mut RunNode,
        id_registry: &mut RunIdRegistry,
        delete_from: Length,
    ) -> Option<RunNode> {
        let inode = match node {
            Node::Internal(inode) => inode,

            Node::Leaf(edit_run) => {
                return edit_run
                    .delete_from(delete_from, id_registry)
                    .map(Node::Leaf);
            },
        };

        let mut start_idx = 0;

        let mut extra = None;

        let mut offset = 0;

        let mut children = inode.children_mut().iter_mut();

        for (idx, child) in children.by_ref().enumerate() {
            let child_len = child.summary().len;

            if offset + child_len >= delete_from {
                start_idx = idx;
                extra =
                    something_start(child, id_registry, delete_from - offset);
                break;
            } else {
                offset += child_len;
            }
        }

        for child in children {
            delete_node(child);
        }

        *inode.summary_mut() = inode.summarize();

        extra.and_then(|e| inode.insert(start_idx + 1, e)).map(Node::Internal)
    }

    fn something_end(
        node: &mut RunNode,
        id_registry: &mut RunIdRegistry,
        delete_up_to: Length,
    ) -> Option<RunNode> {
        let inode = match node {
            Node::Internal(inode) => inode,

            Node::Leaf(edit_run) => {
                return edit_run
                    .delete_up_to(delete_up_to, id_registry)
                    .map(Node::Leaf);
            },
        };

        let mut end_idx = 0;

        let mut extra = None;

        let mut offset = 0;

        for (idx, child) in inode.children_mut().iter_mut().enumerate() {
            let child_len = child.summary().len;

            if offset + child_len >= delete_up_to {
                end_idx = idx;
                extra =
                    something_end(child, id_registry, delete_up_to - offset);
                break;
            } else {
                delete_node(child);
                offset += child_len;
            }
        }

        *inode.summary_mut() = inode.summarize();

        extra.and_then(|e| inode.insert(end_idx + 1, e)).map(Node::Internal)
    }

    fn delete_node(node: &mut RunNode) {
        match node {
            Node::Internal(inode) => {
                inode.summary_mut().len = 0;
            },

            Node::Leaf(edit_run) => edit_run.delete(),
        }
    }
}
