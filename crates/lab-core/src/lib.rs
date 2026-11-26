//! Deterministic simulation kernel: state, topology, deltas, rules, scheduler.
#![forbid(unsafe_code)]
#![deny(clippy::float_arithmetic)]

mod apply;
mod codec;
pub mod config;
mod delta;
mod error;
mod rule;
mod sched;
mod state;
mod topology;

pub use codec::{decode, encode, state_hash, FORMAT_VERSION};
pub use apply::apply;
pub use delta::{check_conservation, leaked_amount, Delta, Holder};
pub use error::LabError;
pub use rule::Rule;
pub use sched::{Engine, CHUNK_SITES};
pub use state::{EntityMeta, Gene, GeneKind, Ledger, SiteIndex, State, SENSE_ALWAYS};
pub use topology::Topology;
