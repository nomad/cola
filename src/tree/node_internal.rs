use super::{Leaf, Metric, Node};

#[derive(Clone)]
pub(super) struct Inode<const N: usize, L: Leaf> {
    children: Vec<Node<N, L>>,
    len: usize,
    summary: L::Summary,
}

impl<const N: usize, L: Leaf> core::fmt::Debug for Inode<N, L> {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        pretty_print_inode(self, &mut String::new(), "", 0, f)
    }
}

impl<const N: usize, L: Leaf> Inode<N, L> {
    #[inline]
    pub(super) fn children(&self) -> &[Node<N, L>] {
        &self.children
    }

    #[inline]
    pub(super) fn children_mut(&mut self) -> &mut [Node<N, L>] {
        &mut self.children
    }

    #[inline]
    pub(super) fn len(&self) -> usize {
        self.children.len()
    }

    #[inline]
    pub(super) fn measure<M: Metric<L>>(&self) -> M {
        M::measure(self.summary())
    }

    #[inline]
    pub(super) fn summary(&self) -> &L::Summary {
        &self.summary
    }
}

/// Recursively prints a tree-like representation of this node.
#[inline]
fn pretty_print_inode<const N: usize, L: Leaf>(
    inode: &Inode<N, L>,
    shifts: &mut String,
    ident: &str,
    last_shift_byte_len: usize,
    f: &mut core::fmt::Formatter,
) -> core::fmt::Result {
    writeln!(
        f,
        "{}{}{:?}",
        &shifts[..shifts.len() - last_shift_byte_len],
        ident,
        inode.summary()
    )?;

    for (i, child) in inode.children().iter().enumerate() {
        let is_last = i + 1 == inode.len();
        let ident = if is_last { "└── " } else { "├── " };
        match child {
            Node::Internal(inode) => {
                let shift = if is_last { "    " } else { "│   " };
                shifts.push_str(shift);
                pretty_print_inode(inode, shifts, ident, shift.len(), f)?;
                shifts.truncate(shifts.len() - shift.len());
            },
            Node::Leaf(leaf) => {
                writeln!(f, "{}{}{:#?}", &shifts, ident, &leaf)?;
            },
        }
    }

    Ok(())
}
