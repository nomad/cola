mod clocks;
mod crdt_edit;
mod fragment;
mod replica;
mod text_edit;

#[cfg(feature = "serde")]
mod serde;

use clocks::{LamportClock, LamportTimestamp, LocalClock, LocalTimestamp};
pub use crdt_edit::CrdtEdit;
use fragment::Fragment;
pub use replica::Replica;
use replica::{EditId, ReplicaId};
pub use text_edit::TextEdit;
