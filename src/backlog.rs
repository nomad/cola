use crate::{Replica, TextEdit};

#[derive(Debug, Clone, Default)]
pub struct BackLog;

impl BackLog {
    /// Creates a new, empty `BackLog`.
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }
}

/// An iterator over the backlogged edits that are now ready to be applied to a
/// `Replica`.
///
/// This struct is created by the [`backlogged`](Replica::backlogged) method on
/// [`Replica`]. See its documentation for more.
pub struct BackLogged<'a> {
    #[allow(unused)]
    replica: &'a mut Replica,
}

impl<'a> BackLogged<'a> {
    pub(crate) fn from_replica(replica: &'a mut Replica) -> Self {
        Self { replica }
    }
}

impl Iterator for BackLogged<'_> {
    type Item = TextEdit;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        todo!();
    }
}
