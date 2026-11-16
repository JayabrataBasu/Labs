//! Deterministic simulation kernel: state, topology, deltas, rules, scheduler.
#![forbid(unsafe_code)]
#![deny(clippy::float_arithmetic)]

mod codec;
mod delta;
mod error;
mod rule;
mod sched;
mod state;
mod topology;

pub use codec::{decode, encode, state_hash, FORMAT_VERSION};
pub use delta::{apply, check_conservation, Delta, Holder};
pub use error::LabError;
pub use rule::Rule;
pub use sched::{Engine, CHUNK_SITES};
pub use state::{EntityMeta, Ledger, SiteIndex, State};
pub use topology::Topology;
