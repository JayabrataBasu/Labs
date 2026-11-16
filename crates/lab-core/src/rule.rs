//! The interface every world, ecological or evolutionary mechanism implements.
use crate::{Delta, SiteIndex, State};
use lab_rng::RuleRng;
use std::ops::Range;

/// A stateless, versioned mechanism. It reads the state and proposes deltas; it never mutates.
pub trait Rule: Send + Sync {
    /// Stable identifier, e.g. `"uptake.fixed"`.
    fn id(&self) -> &'static str;
    /// Bumped whenever behaviour changes, so a manifest pins exact semantics.
    fn version(&self) -> u32;
    /// This instance's random stream; unique within a run and never `STREAM_INIT`.
    fn stream(&self) -> u16;
    /// Pushes deltas for the sites in `sites` only, in a deterministic order.
    ///
    /// Called concurrently for disjoint site ranges; the scheduler concatenates the outputs
    /// in ascending site-range order before applying them.
    fn propose(&self, state: &State, index: &SiteIndex, rng: RuleRng, sites: Range<u32>, out: &mut Vec<Delta>);
}
