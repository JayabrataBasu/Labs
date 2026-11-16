//! The only ways a rule can change the world.
//!
//! Every resource change is a `Transfer` between two holders, so a quantity can only move,
//! enter from `Source`, or leave to `Sink`. Conservation therefore holds by construction,
//! and `check_conservation` verifies it exactly anyway (DR-004). How deltas are applied is
//! defined in `apply.rs`.
use crate::{LabError, State};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Holder {
    Site(u32),
    /// Index into `State::entities`, valid for the tick in which the delta was proposed.
    Entity(u32),
    /// Outside world supplying resource (recorded as influx). Unlimited.
    Source,
    /// Outside world absorbing resource (recorded as dissipation).
    Sink,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Delta {
    /// Move up to `amount` of `resource`; clipped to what `from` holds when the delta is applied.
    Transfer { from: Holder, to: Holder, resource: u16, amount: u64 },
    /// Relocate an entity to a site adjacent to its current one.
    Move { entity: u32, to: u32 },
}

/// For every resource: `total == initial_total + influx - dissipated`, exactly.
pub fn check_conservation(state: &State) -> Result<(), LabError> {
    for r in 0..state.k {
        let expected = state.initial_total[r]
            .checked_add(state.ledger.influx[r])
            .ok_or(LabError::Overflow("conservation identity"))?
            .checked_sub(state.ledger.dissipated[r])
            .ok_or(LabError::Overflow("conservation identity"))?;
        let actual = state.total(r)?;
        if actual != expected {
            return Err(LabError::Conservation { resource: r, expected, actual });
        }
    }
    Ok(())
}
