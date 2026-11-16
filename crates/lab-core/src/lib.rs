//! Deterministic simulation kernel: state, topology, deltas, rules, scheduler.
#![forbid(unsafe_code)]
#![deny(clippy::float_arithmetic)]

mod codec;
mod error;
mod state;
mod topology;

pub use codec::{decode, encode, state_hash, FORMAT_VERSION};
pub use error::LabError;
pub use state::{EntityMeta, Ledger, SiteIndex, State};
pub use topology::Topology;
