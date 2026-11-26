//! The only ways a rule can change the world.
//!
//! Every resource change is a `Transfer` between two holders, so a quantity can only move,
//! enter from `Source`, or leave to `Sink`. Conservation therefore holds by construction,
//! and `check_conservation` verifies it exactly anyway (DR-004). How deltas are applied is
//! defined in `apply.rs`.
use crate::{Gene, LabError, State};

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

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Delta {
    /// Move up to `amount` of `resource`; clipped to what `from` holds when the delta is applied.
    Transfer { from: Holder, to: Holder, resource: u16, amount: u64 },
    /// Inside one entity, consume up to `amount` of `from` and produce
    /// `floor(consumed * yield_num / yield_den)` of `to`. Requires `yield_num <= yield_den`, `yield_den > 0`.
    /// Of that product, a share `leak_ppm` escapes at once (see [`leaked_amount`]) to `leak_to`:
    /// the entity's site, another holder at that site (its host), or the sink.
    Convert { entity: u32, from: u16, to: u16, amount: u64, yield_num: u32, yield_den: u32, leak_ppm: u32, leak_draw: u32, leak_to: Holder },
    /// Relocate an entity to a site adjacent to its current one.
    Move { entity: u32, to: u32 },
    /// Relocate an entity to any site (used by well-mixed control worlds).
    Relocate { entity: u32, to: u32 },
    /// The entity ceases to exist. With `recycle` what it holds returns to its site; without, it
    /// leaves the system as dissipation.
    Remove { entity: u32, recycle: bool },
    /// The parent divides: the offspring appears at the parent's site with half of each of
    /// the parent's stocks (rounded down) and the given genome. The offspring of a contained
    /// entity is contained in the same host.
    Spawn { parent: u32, genome: Vec<Gene> },
    /// `guest` becomes contained in `host`, which must be a free entity at the same site. A guest
    /// that already has a host moves to the new one. An entity that has guests cannot be a guest.
    Attach { guest: u32, host: u32 },
    /// A contained entity becomes free.
    Detach { guest: u32 },
}

/// How much of `produced` leaks at `leak_ppm` parts per million. The whole part always leaks;
/// the fractional unit leaks with the matching probability, decided by `draw`, so that small
/// amounts are not rounded away and the expected leak is exact.
pub fn leaked_amount(produced: u64, leak_ppm: u32, draw: u32) -> u64 {
    const PPM: u128 = 1_000_000;
    let scaled = u128::from(produced) * u128::from(leak_ppm);
    let whole = (scaled / PPM) as u64;
    let extra = u128::from(lab_rng::below(draw, PPM as u32)) < scaled % PPM;
    whole + u64::from(extra)
}

/// For every resource: `total == initial_total + influx + converted_in - dissipated - converted_out`, exactly.
pub fn check_conservation(state: &State) -> Result<(), LabError> {
    for r in 0..state.k {
        let expected = state.initial_total[r]
            .checked_add(state.ledger.influx[r])
            .and_then(|v| v.checked_add(state.ledger.converted_in[r]))
            .and_then(|v| v.checked_sub(state.ledger.dissipated[r]))
            .and_then(|v| v.checked_sub(state.ledger.converted_out[r]))
            .ok_or(LabError::Overflow("conservation identity"))?;
        let actual = state.total(r)?;
        if actual != expected {
            return Err(LabError::Conservation { resource: r, expected, actual });
        }
    }
    Ok(())
}
