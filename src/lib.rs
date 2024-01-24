//! cola is a Conflict-free Replicated Data Type ([CRDT]) for real-time
//! collaborative editing of plain text documents.
//!
//! CRDTs can be roughly divided into two categories: state-based and
//! operation-based. cola falls in the latter category.
//!
//! The basic idea behind an Operation-based CRDT -- also known as a
//! *Commutative* Replicated Data Type or C*m*RDT -- is to design the core data
//! structure and the operations applied to it in such a way that they
//! *commute*, so that the order in which they're applied at each peer doesn't
//! matter.
//!
//! Commutativity makes the final state of the data structure only a function
//! of its initial state and the *set* of operations applied to it, but *not*
//! of the order in which they were applied. This ensures that once all peers
//! have received all operations from all other peers they're guaranteed to
//! converge to the same final state.
//!
//! In cola, the core data structure which represents the state of the document
//! at each peer is the [`Replica`], and the operations which the peers
//! exchange to communicate their local edits are [`Insertion`]s and
//! [`Deletion`]s.
//!
//! If you're new to this crate, reading the documentations of the
//! [`Replica`] struct and its methods would be a good place to start.
//!
//! For a deeper dive into cola's design and implementation you can check out
//! [this blog post][cola].
//!
//! # Example usage
//!
//! ```rust
//! use std::ops::Range;
//!
//! use cola::{Deletion, Replica, ReplicaId};
//!
//! struct Document {
//!     buffer: String,
//!     crdt: Replica,
//! }
//!
//! struct Insertion {
//!     text: String,
//!     crdt: cola::Insertion,
//! }
//!
//! impl Document {
//!     fn new<S: Into<String>>(text: S, replica_id: ReplicaId) -> Self {
//!         let buffer = text.into();
//!         let crdt = Replica::new(replica_id, buffer.len());
//!         Document { buffer, crdt }
//!     }
//!
//!     fn fork(&self, new_replica_id: ReplicaId) -> Self {
//!         let crdt = self.crdt.fork(new_replica_id);
//!         Document { buffer: self.buffer.clone(), crdt }
//!     }
//!
//!     fn insert<S: Into<String>>(
//!         &mut self,
//!         insert_at: usize,
//!         text: S,
//!     ) -> Insertion {
//!         let text = text.into();
//!         self.buffer.insert_str(insert_at, &text);
//!         let insertion = self.crdt.inserted(insert_at, text.len());
//!         Insertion { text, crdt: insertion }
//!     }
//!
//!     fn delete(&mut self, range: Range<usize>) -> Deletion {
//!         self.buffer.replace_range(range.clone(), "");
//!         self.crdt.deleted(range)
//!     }
//!
//!     fn integrate_insertion(&mut self, insertion: Insertion) {
//!         if let Some(offset) =
//!             self.crdt.integrate_insertion(&insertion.crdt)
//!         {
//!             self.buffer.insert_str(offset, &insertion.text);
//!         }
//!     }
//!
//!     fn integrate_deletion(&mut self, deletion: Deletion) {
//!         let ranges = self.crdt.integrate_deletion(&deletion);
//!         for range in ranges.into_iter().rev() {
//!             self.buffer.replace_range(range, "");
//!         }
//!     }
//! }
//!
//! fn main() {
//!     let mut peer_1 = Document::new("Hello, world", 1);
//!     let mut peer_2 = peer_1.fork(2);
//!
//!     let delete_comma = peer_1.delete(5..6);
//!     let insert_exclamation = peer_2.insert(12, "!");
//!
//!     peer_1.integrate_insertion(insert_exclamation);
//!     peer_2.integrate_deletion(delete_comma);
//!
//!     assert_eq!(peer_1.buffer, "Hello world!");
//!     assert_eq!(peer_2.buffer, "Hello world!");
//! }
//! ```
//!
//! # Feature flags
//!
//! - `encode`: enables the [`encode`](Replica::encode) and
//! [`decode`](Replica::decode) methods on [`Replica`] (disabled by default);
//!
//! - `serde`: enables the [`Serialize`] and [`Deserialize`] impls for
//! [`Insertion`], [`Deletion`] and [`EncodedReplica`] (disabled by default).
//!
//! [CRDT]: https://en.wikipedia.org/wiki/Conflict-free_replicated_data_type
//! [cola]: https://nomad.foo/blog/cola
//! [`Serialize`]: https://docs.rs/serde/latest/serde/trait.Serialize.html
//! [`Deserialize`]: https://docs.rs/serde/latest/serde/trait.Deserialize.html

#![allow(clippy::explicit_auto_deref)]
#![allow(clippy::module_inception)]
#![allow(clippy::needless_doctest_main)]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]

extern crate alloc;

mod anchor;
mod backlog;
mod deletion;
#[cfg(feature = "encode")]
mod encode;
#[cfg(feature = "encode")]
mod encoded_replica;
mod gtree;
mod insertion;
mod replica;
mod replica_id;
mod run_indices;
mod run_tree;
mod text;
mod utils;
mod version_map;

use anchor::*;
pub use anchor::{Anchor, AnchorBias};
use backlog::Backlog;
pub use backlog::{BackloggedDeletions, BackloggedInsertions};
pub use deletion::Deletion;
#[cfg(feature = "serde")]
use encode::{impl_deserialize, impl_serialize};
#[cfg(feature = "encode")]
use encode::{Decode, Encode, Int};
#[cfg(feature = "encode")]
use encoded_replica::{checksum, checksum_array};
#[cfg(feature = "encode")]
pub use encoded_replica::{DecodeError, EncodedReplica};
use gtree::{Gtree, LeafIdx};
pub use insertion::Insertion;
pub use replica::Replica;
use replica::*;
pub use replica_id::ReplicaId;
use replica_id::{ReplicaIdMap, ReplicaIdMapValuesMut};
use run_indices::RunIndices;
use run_tree::*;
pub use text::Text;
use utils::*;
use version_map::{DeletionMap, VersionMap};

/// The version of the protocol cola uses to represent `EncodedReplica`s and
/// `CrdtEdit`s.
///
/// You can think of this as a version number for cola's internal
/// representation of the subset of its data structures that are exchanged
/// between peers.
///
/// If different peers are using versions of cola with the same protocol number
/// they're compatible. If not, decoding `EncodedReplica`s and deserializing
/// `CrdtEdit`s will fail.
///
/// # Protocol stability
///
/// cola is still far away from reaching stability, and until that happens its
/// internal `ProtocolVersion` might change very frequently. After the 1.0
/// release the protocol version will only be allowed to be incremented in
/// major releases (if at all).
pub type ProtocolVersion = u64;

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
/// # use cola::Replica;
/// fn insert_at_codepoint(s: &mut String, offset: usize, insert: &str) {
///     let byte_offset = s.chars().take(offset).map(char::len_utf8).sum();
///     s.insert_str(byte_offset, insert);
/// }
///
/// // Peer 1 uses a String as its buffer and codepoints as its unit of
/// // length.
/// let mut buf1 = String::from("àc");
/// let mut replica1 = Replica::new(1, 2); // "àc" has 2 codepoints.
///
/// let mut buf2 = buf1.clone();
/// let mut replica2 = replica1.fork(2);
///
/// // Peer 1 inserts a 'b' between 'à' and 'c' and sends the edit over to the
/// // other peer.
/// let b = "b";
/// insert_at_codepoint(&mut buf1, 1, b);
/// let insert_b = replica1.inserted(1, 1);
///
/// // Peer 2 receives the edit.
/// let offset = replica2.integrate_insertion(&insert_b).unwrap();
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
/// # use cola::Replica;
/// # let b = "b";
/// # let mut buf2 = String::from("àc");
/// # let mut replica1 = Replica::new(1, 2);
/// # let mut replica2 = replica1.fork(2);
/// # let insert_b = replica1.inserted(1, 1);
/// // ..same as before.
///
/// assert_eq!(buf2, "àc");
///
/// // Peer 2 receives the edit.
/// let offset = replica2.integrate_insertion(&insert_b).unwrap();
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
pub type Length = usize;

/// The protocol version of the current cola release.
///
/// See [`ProtocolVersion`] for more infos.
#[cfg(feature = "encode")]
const PROTOCOL_VERSION: ProtocolVersion = 1;
