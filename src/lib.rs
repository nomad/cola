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

mod btree;
mod clocks;
mod crdt_edit;
mod fragment;
mod metrics;
mod node;
mod replica;
mod text_edit;

#[cfg(feature = "serde")]
mod serde;

use btree::Tree;
use clocks::{LamportClock, LamportTimestamp, LocalClock, LocalTimestamp};
pub use crdt_edit::CrdtEdit;
use fragment::{Fragment, FragmentSummary, RunId};
use metrics::ByteMetric;
use replica::EditId;
pub use replica::Replica;
pub use text_edit::TextEdit;
