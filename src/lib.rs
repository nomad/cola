//! cola is a Conflict-free Replicated Data Type ([CRDT]) specialized for
//! real-time collaborative editing of plain text documents.
//!
//! CRDTs can be roughly divided into two categories: state-based and
//! operation-based. cola falls in the latter category.
//!
//! The basic idea behind an Operation-based CRDT (also known as a
//! *Commutative* Replicated Data Type or CmRDT) is to design the core data
//! structure and the operations applied to it in such a way that they
//! *commute*, i.e. the order in which they're applied doesn't matter.
//!
//! Commutativity makes the final state of the data structure only a function
//! of its initial state and the *set* of operations applied to it, but *not*
//! of the order in which they were applied.
//!
//! In turn, this ensures *eventual consistency*, meaning that once all peers
//! have received all operations from all other peers they're guaranteed to
//! converge to the same final state.
//!
//! In cola, the core data structure which represents the state of the document
//! at each peer is the [`Replica`], and the operations which the peers
//! exchange to communicate their local edits are represented by [`CrdtEdit`]s.
//!
//! If you're new to this crate, reading the docs for those two structs would
//! be a good place to start.
//!
//! For a deeper dive into cola's design and implementation you can check out
//! [this blog post][cola].
//!
//! # Code tour of cola's API
//!
//! ```
//! # use cola::{Replica, TextEdit};
//! ```
//!
//! # Feature flags
//!
//! - `serde`: enables the [`Serialize`] and [`Deserialize`] impls for
//! `EncodedReplica` and `CrdtEdit` (disabled by default).
//!
//! [CRDT]: https://en.wikipedia.org/wiki/Conflict-free_replicated_data_type
//! [cola]: https://www.nomad.foo/blog/cola
//! [`Serialize`]: https://docs.rs/serde/latest/serde/trait.Serialize.html
//! [`Deserialize`]: https://docs.rs/serde/latest/serde/trait.Deserialize.html

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
mod utils;
mod version_map;

use backlog::BackLog;
pub use backlog::BackLogged;
pub use crdt_edit::CrdtEdit;
use crdt_edit::{CrdtEditKind, Deletion, Insertion};
use gtree::{Gtree, LeafIdx};
pub use replica::Replica;
use replica::*;
pub use replica_id::ReplicaId;
use replica_id::{ReplicaIdMap, ReplicaIdMapValuesMut};
use run_indices::RunIndices;
use run_tree::*;
pub use text_edit::{Text, TextEdit};
use utils::*;
use version_map::{DeletionMap, VersionMap};

#[cfg(feature = "encode")]
mod encode;
#[cfg(feature = "encode")]
use encode::{checksum, Checksum};
#[cfg(feature = "encode")]
pub use encode::{DecodeError, EncodedReplica};

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
pub type Length = usize;

/// TODO: docs
#[allow(dead_code)]
const PROTOCOL_VERSION: ProtocolVersion = 0;
