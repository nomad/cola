//! Design decisions:
//!
//! a) separate the current buffer contents from the CRDT machinery. Having a
//! `Replica<B: Buffer>` sounds cool in theory but it's actually annoying, e.g.
//! it forces you to give up ownership of your buffer (you can get it back via
//! Deref but still, not clean) and if you want to use a third party buffer w/
//! cola (e.g. crop, Ropey, etc..) you'd need to wrap it in a newtype struct to
//! get around the orphan rule to implement `Buffer` on it;
//!
//! b) the API I'd like to have is something like:
//!
//! - user inserts text,
//!
//! - first they apply it locally to their byffer,
//!
//! - then they feed it to some struct we expose. It's not actually a `Replica`
//! because it doesn't hold the buffer contents, it's more of an `Engine` I
//! guess? I'm still not sure about the name.
//!
//! Its job is to
//!
//! 1. convert edits expressed in local coordinates to CRDT coordinates, e.g.
//!    "insert 'a' at offset 100" to "insert 'a' with edit id `(wen23n13k, 98)`
//!    at offset 50 of insertion `(jjqj91821, 34)`".
//!
//! 2. do the opposite, i.e. get a CRDT coordinate coming from another peer
//!    (which might be a collegue on another computer on the other side of the
//!    world or a syntax plugin running in a separate thread) and convert it to
//!    a sensible edit we can perform (i.e. insert 'b' at offset 69). For this
//!    to always work it's important that everytime the user of the library
//!    edits their buffer they also update the struct we expose, otherwise
//!    they'll get out of sync.
//!
//! I mean that's really the meat of it.
//!
//! The crates has two main structs
//!
//! `Engine/Replica/whatever`: it sits between the local contents of the
//! current buffer and the network/thread/whatever. Its job is just to produce
//! and merge `Edit`s..
//!
//! `Edit`: an opaque struct which we could create by exposing some
//! `.inserted()`, `.deleted()` and `.replaced()` methods on our `Engine`
//! (notice the past tense, it signifies that those methods arent' used to
//! update the contents, they're used to update the `Engine` and get it to
//! create `Edit`s), and it's consumed by another replica via the `.merge()`
//! (name up for discussion) method, which I guess should produce another
//! Edit-like object, except this one should be intelligible, meaning it can be
//! used by the user of their library to update their buffer (maybe a
//! `PlainEdit`?).
//!
//! Another property which I really care about is that (unlike Teletype but
//! like Yjs and diamond-types) everything should be run-length encoded,
//! meaning that if the user inserts (a -> b -> c) sequentially, that should
//! be stored in a single "edit run" inside of each peer's Engine, instead of
//! having an entry for each insertion. I think this'll massively reduce the
//! memory footprint by at least an order of magnitude.
//!
//! A few questions arise:
//!
//! a) how do we handle undo/redo? Should we leave that to the user (we
//! wouldn't want an `undo` operation to undo someone else's changes)? If so we
//! should allow to reverse an `Edit` to get its inverse;
//!
//! b) how do we send an `Engine` over to another peer? Imagine you've been
//! editing with a collegue for a while when a third person joins the call. One
//! of you two (probably the one that either invited or accepted them into the
//! call) should send them the entire editing history, which maybe should mean
//! that the `Engine` itself should be serializable/deserializable? Can we do
//! better?
//!
//! c) do we need to handle the list of peers known by each replica? I imagine
//! that if we try to merge an edit coming from a peer our Engine has never
//! seen before it won't know wtf to do with it? Or maybe it works fine?
//!
//! d) does the `Engine` have to be shared between threads? I don't think so
//! because all it does is create/merge edits and like I'll say below that
//! should be fast enough to do synchronously, whereas with a Rope we could
//! potentially do all sorts of insteresting, long-running
//! computations/analysis. However if we do need to share it we also need to
//! worry about adding copy-on-write semantics.
//!
//! Out of scope for this crate:
//!
//! - async: everything should be fast enough to be performed instantly on the
//! main thread without the need for async. The goal is to process the
//! automerge-paper editing trace in under 35ms on my machine;
//!
//! - IO: the most we do is create those `Edit` structs that can implement
//! `Serialize` and `Deserialize` if the "serde" feature flag is enabled. It's
//! up to the user to send them to the remote peer using the network layer of
//! their choice;
//!
//! - long-running editing sessions: nothing gets saved to disk. I'm not trying
//! to write a collaborative Git. I think diamond-types is because I saw a
//! bunch of disk-related stuff when I briefly looked at its source code. Its 2
//! main objects are also called `OpLog` and `Branch` which feels very vcs-y to
//! me. I ain't doing none of that.
//!
//! - maybe undo/redo but I'm not sure. We probably don't want to handle the
//! "undoing should only undo *my* last operation, not the last operation of
//! another peer" logic, but if we don't we need to allow the end user to do
//! that by storing something like a `HashMap<PeerId, Vec<Edit>>`, and by
//! allowing them to turn each `Edit` in the undo stack into its opposite edit
//! operation based on the current document's coordinates. This probably also
//! means that feeding the same `Edit` to `Replica::merge` doesn't always
//! produce the same `PlainEdit?` because it depends on state of the `Replica`.
//!
//! This honestly feels like it's not that much code. I mean we'll probably
//! have to come up with some fancy data structures to make both the
//! `PlainEdit -> Edit` and `Edit -> PlainEdit` paths really fast, but those
//! and `Replica::merge` are pretty much the only algorithms we have to
//! implement. In comparison crop has a shit ton more things going on: Ropes,
//! RopeSlices, b-tree rebalancing, iterators, RopeBuilders, etc.
//!
//! I'd guess this'll turn out to be 5-7k loc.

#![allow(clippy::explicit_auto_deref)]
#![allow(clippy::module_inception)]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]

extern crate alloc;

mod backlog;
mod crdt_edit;
mod gtree;
mod replica;
mod replica_id;
mod run_indices;
mod run_tree;
mod text_edit;

#[cfg(feature = "serde")]
mod serde;

use backlog::BackLog;
pub use backlog::BackLogged;
pub use crdt_edit::CrdtEdit;
use crdt_edit::CrdtEditKind;
use gtree::{Gtree, LeafIdx};
pub use replica::Replica;
use replica::*;
use replica_id::{ReplicaId, ReplicaIdMap};
use run_indices::RunIndices;
use run_tree::{Anchor, DeletionOutcome, EditRun, InsertionOutcome, RunTree};
pub use text_edit::TextEdit;

/// The length of a piece of text according to some user-defined metric.
///
/// The meaning of a unit of length is decided by you, the user of this
/// library, depending on the kind of buffer you're using cola with. This
/// allows cola to work with buffers using a variety of encodings (UTF-8,
/// UTF-16, etc.) and indexing metrics (bytes, codepoints, graphemes, etc.).
///
/// While the particular meaning of a unit of length is up to the user, it is
/// important that it is consistent across all peers. For example, if one peer
/// uses bytes as its unit of length, all other peers must also use bytes or
/// the contents of their buffers will diverge.
///
/// # Examples
///
/// In this example all peers use the same metric (codepoints) and everything
/// works as expected:
///
/// ```
/// # use cola::{Replica, TextEdit};
/// fn insert_at_codepoint(s: &mut String, offset: usize, s: &str) {
///     let byte_offset = s.chars().take(offset).map(char::len_utf8).sum();
///     s.insert_str(byte_offset, s);
/// }
///
/// // Peer 1 uses a String as its buffer and codepoints as its unit of
/// // length.
/// let mut buf1 = String::from("àc");
/// let mut replica1 = Replica::new(2); // "àc" has 2 codepoints.
///
/// let mut buf2 = buf1.clone();
/// let mut replica2 = replica1.clone();
///
/// // Peer 1 inserts a 'b' between 'à' and 'c' and sends the edit over to the
/// // other peer.
/// let b = "b";
/// insert_at_codepoint(&mut buf1, 1, b);
/// let insert_b = replica1.inserted(1, 1);
///
/// // Peer 2 receives the edit.
/// let Some(TextEdit::Insertion(offset)) = replica2.merge(insert_b) else {
///     unreachable!();
/// };
///
/// assert_eq!(offset, 1);
///
/// // Peer 2 also uses codepoints as its unit of length, so it inserts the
/// // 'b' after the 'à' as expected.
/// insert_at_codepoint(&mut buf2, offset, b);
///
/// // If all the peers use the same metric they'll always converge to the
/// // same state.
/// assert_eq!(buf1, "àbc");
/// assert_eq!(buf2, "àbc");
/// ```
///
/// If different peers use different metrics, however, their buffers can
/// diverge or even cause the program to crash, like in the following example:
///
/// ```should_panic
/// # use cola::{Replica, TextEdit};
/// # let b = "b";
/// # let mut buf2 = String::from("àc");
/// # let mut replica1 = Replica::new(2);
/// # let mut replica2 = replica1.clone();
/// # let insert_b = replica1.inserted(1, 1);
/// // ..same as before.
///
/// assert_eq!(buf2, "àc");
///
/// // Peer 2 receives the edit.
/// let Some(TextEdit::Insertion(offset)) = replica2.merge(insert_b) else {
///     unreachable!();
/// };
///
/// assert_eq!(offset, 1);
///
/// // Now let's say peer 2 interprets `offset` as a byte offset even though
/// // the insertion of the 'b' was done using codepoint offsets on peer 1.
/// //
/// // In this case the program just panics because a byte offset of 1 is not a
/// // valid insertion point in the string "àc" since it falls in the middle of
/// // the 'à' codepoint, which is 2 bytes long.
/// //
/// // In other cases the program might not panic but instead cause the peers
/// // to silently diverge, which is arguably worse.
///
/// buf2.insert_str(offset, b); // 💥 panics!
/// ```
pub type Length = u64;

use range::{Range, RangeExt};

mod range {
    use core::cmp::Ord;
    use core::fmt::{Debug, Formatter, Result as FmtResult};
    use core::ops::{Add, Range as StdRange, Sub};

    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct Range<T> {
        pub start: T,
        pub end: T,
    }

    impl<T: Debug> Debug for Range<T> {
        fn fmt(&self, f: &mut Formatter) -> FmtResult {
            write!(f, "{:?}..{:?}", self.start, self.end)
        }
    }

    impl<T> From<StdRange<T>> for Range<T> {
        #[inline]
        fn from(range: StdRange<T>) -> Self {
            Range { start: range.start, end: range.end }
        }
    }

    impl<T: Sub<T, Output = T> + Copy> Sub<T> for Range<T> {
        type Output = Range<T>;

        #[inline]
        fn sub(self, value: T) -> Self::Output {
            Range { start: self.start - value, end: self.end - value }
        }
    }

    impl<T: Add<T, Output = T> + Copy> Add<T> for Range<T> {
        type Output = Range<T>;

        #[inline]
        fn add(self, value: T) -> Self::Output {
            Range { start: self.start + value, end: self.end + value }
        }
    }

    impl<T> Range<T> {
        #[inline]
        pub fn len(&self) -> T
        where
            T: Sub<T, Output = T> + Copy,
        {
            self.end - self.start
        }
    }

    pub trait RangeExt<T> {
        fn contains_range(&self, range: Range<T>) -> bool;
    }

    impl<T: Ord> RangeExt<T> for StdRange<T> {
        #[inline]
        fn contains_range(&self, other: Range<T>) -> bool {
            self.start <= other.start && self.end >= other.end
        }
    }
}

/// TODO: docs
#[inline]
fn get_two_mut<T>(
    slice: &mut [T],
    first_idx: usize,
    second_idx: usize,
) -> (&mut T, &mut T) {
    debug_assert!(first_idx != second_idx);

    if first_idx < second_idx {
        debug_assert!(second_idx < slice.len());
        let split_at = first_idx + 1;
        let (first, second) = slice.split_at_mut(split_at);
        (&mut first[first_idx], &mut second[second_idx - split_at])
    } else {
        debug_assert!(first_idx < slice.len());
        let split_at = second_idx + 1;
        let (first, second) = slice.split_at_mut(split_at);
        (&mut second[first_idx - split_at], &mut first[second_idx])
    }
}

/// TODO: docs
#[inline]
fn insert_in_slice<T>(slice: &mut [T], elem: T, at_offset: usize) {
    debug_assert!(at_offset < slice.len());
    slice[at_offset..].rotate_right(1);
    slice[at_offset] = elem;
}
