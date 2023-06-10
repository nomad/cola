pub use delete::delete;
pub use insert::insert;

use crate::*;

mod insert {
    //! This module handles the logic used to create [`CrdtEdit`]s after the
    //! user locally inserts some text in their buffer.

    use super::*;

    /// TODO: docs
    pub fn insert(
        btree: &mut RunTree,
        id_registry: &mut RunIdRegistry,
        edit_id: InsertionId,
        lamport_ts: LamportTimestamp,
        byte_offset: usize,
        text_len: usize,
    ) -> InsertionAnchor {
        let root = match btree.root_mut() {
            Node::Internal(inode) => inode,

            Node::Leaf(edit_run) => {
                let (new_run, split_run) = edit_run.insert(
                    edit_id,
                    lamport_ts,
                    byte_offset,
                    text_len,
                    id_registry,
                );

                let anchor = new_run.anchor();

                let new_run = Node::Leaf(new_run);

                btree.replace_root(|first_run| {
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

        let (insertion_id, root_split) = insertedd(
            root,
            id_registry,
            edit_id,
            lamport_ts,
            byte_offset,
            text_len,
        );

        if let Some(root_split) = root_split.map(Node::Internal) {
            btree.replace_root(|old_root| {
                Node::from_children(vec![old_root, root_split])
            });
        }

        insertion_id
    }

    /// TODO: docs
    fn insertedd(
        inode: &mut RunInode,
        id_registry: &mut RunIdRegistry,
        edit_id: InsertionId,
        lamport_ts: LamportTimestamp,
        byte_offset: usize,
        text_len: usize,
    ) -> (InsertionAnchor, Option<RunInode>) {
        let mut offset = 0;

        for (idx, child) in inode.children_mut().iter_mut().enumerate() {
            let child_len = child.summary().len;

            offset += child_len;

            if offset >= byte_offset {
                offset -= child_len;

                if child.is_internal() {
                    let (id, split) = inode.with_child_mut(idx, |child| {
                        insertedd(
                            child.as_internal_mut(),
                            id_registry,
                            edit_id,
                            lamport_ts,
                            byte_offset - offset,
                            text_len,
                        )
                    });

                    let split = split.and_then(|s| {
                        inode.insert(idx + 1, Node::Internal(s))
                    });

                    return (id, split);
                } else {
                    let (new_run, split_run) =
                        inode.with_child_mut(idx, |child| {
                            let edit_run = child.as_leaf_mut();

                            edit_run.insert(
                                edit_id,
                                lamport_ts,
                                byte_offset - offset,
                                text_len,
                                id_registry,
                            )
                        });

                    let anchor = new_run.anchor();

                    let new_run = Node::Leaf(new_run);

                    let offset = idx + 1;

                    let split = if let Some(split_run) =
                        split_run.map(Node::Leaf)
                    {
                        inode.insert_two(offset, new_run, offset, split_run)
                    } else {
                        inode.insert(offset, new_run)
                    };

                    return (anchor, split);
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
        btree: &mut RunTree,
        id_registry: &mut RunIdRegistry,
        byte_range: Range<usize>,
    ) {
        let root = match btree.root_mut() {
            Node::Internal(inode) => inode,

            Node::Leaf(edit_run) => {
                let (deleted, rest) =
                    edit_run.delete_range(byte_range, id_registry);

                if let Some(deleted) = deleted.map(Node::Leaf) {
                    btree.replace_root(|old_root| {
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
            deleted(root, id_registry, byte_range).map(Node::Internal)
        {
            btree.replace_root(|old_root| {
                Node::from_children(vec![old_root, root_split])
            });
        }
    }

    fn deleted(
        inode: &mut RunInode,
        id_registry: &mut RunIdRegistry,
        mut byte_range: Range<usize>,
    ) -> Option<RunInode> {
        let mut offset = 0;

        for (idx, child) in inode.children().iter().enumerate() {
            let child_len = child.summary().len;

            offset += child_len;

            if offset < byte_range.start {
                continue;
            }

            if offset >= byte_range.end {
                offset -= child_len;

                byte_range.start -= offset;
                byte_range.end -= offset;

                if child.is_internal() {
                    let split = inode.with_child_mut(idx, |child| {
                        let child = child.as_internal_mut();
                        deleted(child, id_registry, byte_range)
                    });

                    return split.and_then(|s| {
                        inode.insert(idx + 1, Node::Internal(s))
                    });
                } else {
                    let (deleted, rest) = inode.with_child_mut(idx, |child| {
                        let edit_run = child.as_leaf_mut();
                        edit_run.delete_range(byte_range, id_registry)
                    });

                    let deleted = deleted.map(Node::Leaf)?;

                    let offset = idx + 1;

                    return if let Some(rest) = rest.map(Node::Leaf) {
                        inode.insert_two(offset, deleted, offset, rest)
                    } else {
                        inode.insert(offset, deleted)
                    };
                }
            } else {
                return delete_range_in_deepest(
                    inode,
                    id_registry,
                    byte_range,
                );
            }
        }

        unreachable!();
    }

    fn delete_range_in_deepest(
        inode: &mut RunInode,
        id_registry: &mut RunIdRegistry,
        delete_range: Range<usize>,
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

            if offset >= delete_range.start {
                start_idx = idx;
                let delete_from = delete_range.start + child_len - offset;
                extra_from_start =
                    something_start(child, id_registry, delete_from);
                break;
            }
        }

        for (idx, child) in children {
            let child_len = child.summary().len;

            offset += child_len;

            if offset >= delete_range.end {
                end_idx = idx;
                let delete_up_to = delete_range.end + child_len - offset;
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
        delete_from: usize,
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
        delete_up_to: usize,
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
