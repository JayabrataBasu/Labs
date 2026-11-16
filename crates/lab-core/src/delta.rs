//! The only ways a rule can change the world.
//!
//! Every resource change is a `Transfer` between two holders, so a quantity can only move,
//! enter from `Source`, or leave to `Sink`. Conservation therefore holds by construction,
//! and `check_conservation` verifies it exactly anyway (DR-004).
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

/// Applies deltas strictly in slice order. On error the offending delta has changed nothing.
pub fn apply(state: &mut State, deltas: &[Delta]) -> Result<(), LabError> {
    for delta in deltas {
        match *delta {
            Delta::Transfer { from, to, resource, amount } => transfer(state, from, to, resource as usize, amount)?,
            Delta::Move { entity, to } => {
                let n_sites = state.topo.n_sites();
                let Some(meta) = state.entities.get(entity as usize) else {
                    return Err(LabError::Config(format!("move refers to entity index {entity}, which does not exist")));
                };
                if to >= n_sites || !state.topo.neighbours(meta.site).contains(&to) {
                    return Err(LabError::Config(format!("entity {} cannot move from site {} to non-adjacent site {to}", meta.id, meta.site)));
                }
                state.entities[entity as usize].site = to;
            }
        }
    }
    Ok(())
}

/// Where a holder's quantity lives.
enum Slot {
    Site(usize),
    Entity(usize),
}

fn slot(state: &State, holder: Holder, r: usize) -> Result<Option<Slot>, LabError> {
    match holder {
        Holder::Site(s) if s < state.topo.n_sites() => Ok(Some(Slot::Site(s as usize * state.k + r))),
        Holder::Site(s) => Err(LabError::Config(format!("transfer refers to site {s}, which does not exist"))),
        Holder::Entity(e) if (e as usize) < state.entities.len() => Ok(Some(Slot::Entity(e as usize * state.k + r))),
        Holder::Entity(e) => Err(LabError::Config(format!("transfer refers to entity index {e}, which does not exist"))),
        Holder::Source | Holder::Sink => Ok(None),
    }
}

fn transfer(state: &mut State, from: Holder, to: Holder, r: usize, amount: u64) -> Result<(), LabError> {
    if r >= state.k {
        return Err(LabError::Config(format!("transfer refers to resource {r}, but there are only {}", state.k)));
    }
    if from == Holder::Sink || to == Holder::Source || (from == Holder::Source && to == Holder::Sink) {
        return Err(LabError::Config(format!("invalid transfer direction {from:?} -> {to:?}")));
    }
    let from_slot = slot(state, from, r)?;
    let to_slot = slot(state, to, r)?;
    if from == to {
        return Ok(());
    }
    let read = |state: &State, s: &Slot| match *s {
        Slot::Site(i) => state.site_stock[i],
        Slot::Entity(i) => state.ent_stock[i],
    };
    let moved = match &from_slot {
        Some(s) => amount.min(read(state, s)),
        None => amount,
    };
    // Compute every new value first, so an overflow leaves the state untouched.
    let new_to = match &to_slot {
        Some(s) => Some(read(state, s).checked_add(moved).ok_or(LabError::Overflow("resource stock"))?),
        None => None,
    };
    let new_influx = match from {
        Holder::Source => Some(state.ledger.influx[r].checked_add(moved).ok_or(LabError::Overflow("influx ledger"))?),
        _ => None,
    };
    let new_dissipated = match to {
        Holder::Sink => Some(state.ledger.dissipated[r].checked_add(moved).ok_or(LabError::Overflow("dissipation ledger"))?),
        _ => None,
    };
    let write = |state: &mut State, s: &Slot, v: u64| match *s {
        Slot::Site(i) => state.site_stock[i] = v,
        Slot::Entity(i) => state.ent_stock[i] = v,
    };
    if let Some(s) = &from_slot {
        let v = read(state, s) - moved;
        write(state, s, v);
    }
    if let (Some(s), Some(v)) = (&to_slot, new_to) {
        write(state, s, v);
    }
    if let Some(v) = new_influx {
        state.ledger.influx[r] = v;
    }
    if let Some(v) = new_dissipated {
        state.ledger.dissipated[r] = v;
    }
    Ok(())
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
