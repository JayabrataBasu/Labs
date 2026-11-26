//! Environmental rules beyond the M1 set.
//!
//! - `decay.fraction`: every site loses `stock / divisor` of a resource (signals fade, goods spoil).
//! - `mix.relocate`: with a fixed probability an entity jumps to a uniformly random site.
//!   This removes spatial structure: null world N2 (well-mixed).
//! - `shock.scheduled`: at one tick, add to or remove from the sites of a rectangle. A declared,
//!   reproducible perturbation.
//! - `shock.cull`: at one tick, each entity is removed with a fixed probability.
use crate::{invalid, params, resource_index};
use lab_core::config::RuleCfg;
use lab_core::{Delta, Holder, LabError, Rule, SiteIndex, State};
use lab_rng::{below, RuleRng};
use serde::Deserialize;
use std::ops::Range;

const PPM: u32 = 1_000_000;

macro_rules! rule_header {
    ($id:expr) => {
        fn id(&self) -> &'static str {
            $id
        }
        fn version(&self) -> u32 {
            1
        }
        fn stream(&self) -> u16 {
            self.stream
        }
    };
}

// ---- decay.fraction -----------------------------------------------------------------------

pub const DECAY_ID: &str = "decay.fraction";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DecayParams {
    resource: String,
    divisor: u64,
}

pub struct Decay {
    stream: u16,
    resource: u16,
    divisor: u64,
}

impl Decay {
    pub fn new(cfg: &RuleCfg, resources: &[String]) -> Result<Self, LabError> {
        let p: DecayParams = params(cfg)?;
        if p.divisor == 0 {
            return Err(invalid(cfg, "divisor must be at least 1"));
        }
        Ok(Decay { stream: cfg.stream, resource: resource_index(cfg, &p.resource, resources)?, divisor: p.divisor })
    }
}

impl Rule for Decay {
    rule_header!(DECAY_ID);
    fn propose(&self, state: &State, _: &SiteIndex, _: RuleRng, sites: Range<u32>, out: &mut Vec<Delta>) {
        for site in sites {
            let amount = state.site_stock[site as usize * state.k + self.resource as usize] / self.divisor;
            if amount > 0 {
                out.push(Delta::Transfer { from: Holder::Site(site), to: Holder::Sink, resource: self.resource, amount });
            }
        }
    }
}

// ---- mix.relocate -------------------------------------------------------------------------

pub const RELOCATE_ID: &str = "mix.relocate";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RelocateParams {
    probability_ppm: u32,
}

pub struct Relocate {
    stream: u16,
    probability_ppm: u32,
}

impl Relocate {
    pub fn new(cfg: &RuleCfg) -> Result<Self, LabError> {
        let p: RelocateParams = params(cfg)?;
        if p.probability_ppm > PPM {
            return Err(invalid(cfg, "probability_ppm must be between 0 and 1000000"));
        }
        Ok(Relocate { stream: cfg.stream, probability_ppm: p.probability_ppm })
    }
}

impl Rule for Relocate {
    rule_header!(RELOCATE_ID);
    fn propose(&self, state: &State, index: &SiteIndex, rng: RuleRng, sites: Range<u32>, out: &mut Vec<Delta>) {
        let tick = state.tick as u32;
        let n_sites = state.topo.n_sites();
        for site in sites {
            for &entity in index.at(site) {
                if state.entities[entity as usize].parent.is_some() {
                    continue; // carried by its host
                }
                let draw = rng.draw(tick, state.entities[entity as usize].id, 0);
                if below(draw[0], PPM) < self.probability_ppm {
                    out.push(Delta::Relocate { entity, to: below(draw[1], n_sites) });
                }
            }
        }
    }
}

// ---- shock.scheduled ----------------------------------------------------------------------

pub const SHOCK_ID: &str = "shock.scheduled";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ShockParams {
    tick: u64,
    /// `"add"` or `"remove"`.
    action: String,
    resource: String,
    amount: u64,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

pub struct Shock {
    stream: u16,
    tick: u64,
    add: bool,
    resource: u16,
    amount: u64,
    x: Range<u32>,
    y: Range<u32>,
}

impl Shock {
    pub fn new(cfg: &RuleCfg, resources: &[String], world: (u32, u32)) -> Result<Self, LabError> {
        let p: ShockParams = params(cfg)?;
        let add = match p.action.as_str() {
            "add" => true,
            "remove" => false,
            _ => return Err(invalid(cfg, "action must be \"add\" or \"remove\"")),
        };
        let fits = |start: u32, len: u32, limit: u32| len > 0 && start.checked_add(len).is_some_and(|end| end <= limit);
        if !fits(p.x, p.width, world.0) || !fits(p.y, p.height, world.1) {
            return Err(invalid(cfg, "the rectangle does not lie inside the lattice"));
        }
        Ok(Shock {
            stream: cfg.stream,
            tick: p.tick,
            add,
            resource: resource_index(cfg, &p.resource, resources)?,
            amount: p.amount,
            x: p.x..p.x + p.width,
            y: p.y..p.y + p.height,
        })
    }
}

impl Rule for Shock {
    rule_header!(SHOCK_ID);
    fn propose(&self, state: &State, _: &SiteIndex, _: RuleRng, sites: Range<u32>, out: &mut Vec<Delta>) {
        if state.tick != self.tick {
            return;
        }
        for site in sites {
            let (x, y) = (site % state.topo.width, site / state.topo.width);
            if self.x.contains(&x) && self.y.contains(&y) {
                let (from, to) = if self.add { (Holder::Source, Holder::Site(site)) } else { (Holder::Site(site), Holder::Sink) };
                out.push(Delta::Transfer { from, to, resource: self.resource, amount: self.amount });
            }
        }
    }
}

// ---- shock.cull ---------------------------------------------------------------------------

pub const CULL_ID: &str = "shock.cull";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CullParams {
    tick: u64,
    probability_ppm: u32,
    recycle: bool,
}

pub struct Cull {
    stream: u16,
    tick: u64,
    probability_ppm: u32,
    recycle: bool,
}

impl Cull {
    pub fn new(cfg: &RuleCfg) -> Result<Self, LabError> {
        let p: CullParams = params(cfg)?;
        if p.probability_ppm > PPM {
            return Err(invalid(cfg, "probability_ppm must be between 0 and 1000000"));
        }
        Ok(Cull { stream: cfg.stream, tick: p.tick, probability_ppm: p.probability_ppm, recycle: p.recycle })
    }
}

impl Rule for Cull {
    rule_header!(CULL_ID);
    fn propose(&self, state: &State, index: &SiteIndex, rng: RuleRng, sites: Range<u32>, out: &mut Vec<Delta>) {
        if state.tick != self.tick {
            return;
        }
        for site in sites {
            for &entity in index.at(site) {
                if below(rng.draw(state.tick as u32, state.entities[entity as usize].id, 0)[0], PPM) < self.probability_ppm {
                    out.push(Delta::Remove { entity, recycle: self.recycle });
                }
            }
        }
    }
}
