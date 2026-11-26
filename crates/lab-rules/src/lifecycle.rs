//! Birth and death.
//!
//! - `reproduction.division`: an entity holding at least the threshold of every resource in
//!   `requires` divides, unless its site already holds `site_capacity` entities. The division
//!   first spends `consume` of each required resource (building material, which leaves the
//!   system), so a requirement can only be met again by obtaining more. Selection acts through
//!   this rule: who reproduces depends on what their genes achieved.
//! - `reproduction.random`: every entity divides with a fixed probability, whatever it holds.
//!   With `death.random` this is null world N3 (demography without selection).
//! - `death.starvation`: an entity holding none of a resource is removed.
//! - `death.random`: every entity is removed with a fixed probability.
//!
//! Offspring receive half of each parental stock and a mutated copy of the genome (`mutation.rs`).
use crate::mutation::MutationParams;
use crate::{invalid, params, resource_index};
use lab_core::config::RuleCfg;
use lab_core::{Delta, Holder, LabError, Rule, SiteIndex, State};
use lab_rng::{below, RuleRng};
use serde::Deserialize;
use std::ops::Range;

const PPM: u32 = 1_000_000;
/// Draw index used for the "does this entity reproduce / die" decision (gene positions use 0..4096 and 0x8000..).
const DECISION: u16 = 0x7fff;

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

fn check_ppm(cfg: &RuleCfg, ppm: u32) -> Result<(), LabError> {
    if ppm > PPM {
        return Err(invalid(cfg, "probability_ppm must be between 0 and 1000000"));
    }
    Ok(())
}

/// Places left for offspring at one site. `site_capacity` bounds the free entities on a site and,
/// separately, the guests inside each host (guests take no room on the site itself).
struct Room {
    free: usize,
    capacity: usize,
    /// `(host id, guests it has, counting offspring proposed now)`, in order of first appearance.
    guests: Vec<(u64, usize)>,
}

impl Room {
    fn new(state: &State, residents: &[u32], capacity: u32) -> Self {
        let capacity = capacity as usize;
        let mut guests: Vec<(u64, usize)> = Vec::new();
        let mut contained = 0;
        for host in residents.iter().filter_map(|&e| state.entities[e as usize].parent) {
            contained += 1;
            match guests.iter_mut().find(|g| g.0 == host) {
                Some(g) => g.1 += 1,
                None => guests.push((host, 1)),
            }
        }
        Room { free: capacity.saturating_sub(residents.len() - contained), capacity, guests }
    }
    fn available(&self, state: &State, entity: usize) -> bool {
        match state.entities[entity].parent {
            None => self.free > 0,
            Some(host) => self.guests.iter().any(|g| g.0 == host && g.1 < self.capacity),
        }
    }
    fn take(&mut self, state: &State, entity: usize) {
        match state.entities[entity].parent {
            None => self.free -= 1,
            Some(host) => self.guests.iter_mut().filter(|g| g.0 == host).for_each(|g| g.1 += 1),
        }
    }
}

// ---- reproduction.division ----------------------------------------------------------------

pub const DIVISION_ID: &str = "reproduction.division";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Requirement {
    resource: String,
    threshold: u64,
    consume: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DivisionParams {
    /// All of these must be held before an entity can divide.
    requires: Vec<Requirement>,
    site_capacity: u32,
    mutation: MutationParams,
}

pub struct Division {
    stream: u16,
    /// `(resource index, threshold, consumed)`.
    requires: Vec<(usize, u64, u64)>,
    site_capacity: u32,
    mutation: MutationParams,
}

impl Division {
    pub fn new(cfg: &RuleCfg, resources: &[String]) -> Result<Self, LabError> {
        let p: DivisionParams = params(cfg)?;
        p.mutation.validate().map_err(|e| invalid(cfg, &e.to_string()))?;
        if p.requires.is_empty() || p.requires.iter().any(|r| r.threshold == 0) {
            return Err(invalid(cfg, "requires must list at least one resource, each with a threshold of at least 1"));
        }
        if p.requires.iter().any(|r| r.consume > r.threshold) {
            return Err(invalid(cfg, "consume must not exceed the threshold of the same resource"));
        }
        let mut requires: Vec<(usize, u64, u64)> = Vec::with_capacity(p.requires.len());
        for r in &p.requires {
            let index = resource_index(cfg, &r.resource, resources)? as usize;
            if requires.iter().any(|&(other, ..)| other == index) {
                return Err(invalid(cfg, "requires lists a resource twice"));
            }
            requires.push((index, r.threshold, r.consume));
        }
        Ok(Division {
            stream: cfg.stream,
            requires,
            site_capacity: p.site_capacity,
            mutation: p.mutation,
        })
    }
}

impl Rule for Division {
    fn id(&self) -> &'static str {
        DIVISION_ID
    }
    fn version(&self) -> u32 {
        3
    }
    fn stream(&self) -> u16 {
        self.stream
    }
    fn propose(&self, state: &State, index: &SiteIndex, rng: RuleRng, sites: Range<u32>, out: &mut Vec<Delta>) {
        let tick = state.tick as u32;
        for site in sites {
            let residents = index.at(site);
            let mut room = Room::new(state, residents, self.site_capacity);
            for &entity in residents {
                let e = entity as usize;
                if !room.available(state, e) {
                    continue;
                }
                if self.requires.iter().all(|&(r, threshold, _)| state.ent_stock[e * state.k + r] >= threshold) {
                    for &(r, _, amount) in self.requires.iter().filter(|&&(.., amount)| amount > 0) {
                        out.push(Delta::Transfer { from: Holder::Entity(entity), to: Holder::Sink, resource: r as u16, amount });
                    }
                    let genome = self.mutation.offspring(&state.genomes[e], rng, tick, state.entities[e].id, state.k as u8);
                    out.push(Delta::Spawn { parent: entity, genome });
                    room.take(state, e);
                }
            }
        }
    }
}

// ---- reproduction.random ------------------------------------------------------------------

pub const RANDOM_BIRTH_ID: &str = "reproduction.random";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RandomBirthParams {
    probability_ppm: u32,
    site_capacity: u32,
    mutation: MutationParams,
}

pub struct RandomBirth {
    stream: u16,
    probability_ppm: u32,
    site_capacity: u32,
    mutation: MutationParams,
}

impl RandomBirth {
    pub fn new(cfg: &RuleCfg) -> Result<Self, LabError> {
        let p: RandomBirthParams = params(cfg)?;
        p.mutation.validate().map_err(|e| invalid(cfg, &e.to_string()))?;
        check_ppm(cfg, p.probability_ppm)?;
        Ok(RandomBirth { stream: cfg.stream, probability_ppm: p.probability_ppm, site_capacity: p.site_capacity, mutation: p.mutation })
    }
}

impl Rule for RandomBirth {
    rule_header!(RANDOM_BIRTH_ID);
    fn propose(&self, state: &State, index: &SiteIndex, rng: RuleRng, sites: Range<u32>, out: &mut Vec<Delta>) {
        let tick = state.tick as u32;
        for site in sites {
            let residents = index.at(site);
            let mut room = Room::new(state, residents, self.site_capacity);
            for &entity in residents {
                let e = entity as usize;
                if !room.available(state, e) {
                    continue;
                }
                let id = state.entities[e].id;
                if below(rng.draw(tick, id, DECISION)[0], PPM) < self.probability_ppm {
                    let genome = self.mutation.offspring(&state.genomes[e], rng, tick, id, state.k as u8);
                    out.push(Delta::Spawn { parent: entity, genome });
                    room.take(state, e);
                }
            }
        }
    }
}

// ---- death.starvation ---------------------------------------------------------------------

pub const STARVATION_ID: &str = "death.starvation";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StarvationParams {
    resource: String,
    /// Whether a dead entity's stocks return to its site (true) or leave the system (false).
    recycle: bool,
}

pub struct Starvation {
    stream: u16,
    resource: usize,
    recycle: bool,
}

impl Starvation {
    pub fn new(cfg: &RuleCfg, resources: &[String]) -> Result<Self, LabError> {
        let p: StarvationParams = params(cfg)?;
        Ok(Starvation { stream: cfg.stream, resource: resource_index(cfg, &p.resource, resources)? as usize, recycle: p.recycle })
    }
}

impl Rule for Starvation {
    rule_header!(STARVATION_ID);
    fn propose(&self, state: &State, index: &SiteIndex, _: RuleRng, sites: Range<u32>, out: &mut Vec<Delta>) {
        for site in sites {
            for &entity in index.at(site) {
                if state.ent_stock[entity as usize * state.k + self.resource] == 0 {
                    out.push(Delta::Remove { entity, recycle: self.recycle });
                }
            }
        }
    }
}

// ---- death.random -------------------------------------------------------------------------

pub const RANDOM_DEATH_ID: &str = "death.random";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RandomDeathParams {
    probability_ppm: u32,
    recycle: bool,
}

pub struct RandomDeath {
    stream: u16,
    probability_ppm: u32,
    recycle: bool,
}

impl RandomDeath {
    pub fn new(cfg: &RuleCfg) -> Result<Self, LabError> {
        let p: RandomDeathParams = params(cfg)?;
        check_ppm(cfg, p.probability_ppm)?;
        Ok(RandomDeath { stream: cfg.stream, probability_ppm: p.probability_ppm, recycle: p.recycle })
    }
}

impl Rule for RandomDeath {
    rule_header!(RANDOM_DEATH_ID);
    fn propose(&self, state: &State, index: &SiteIndex, rng: RuleRng, sites: Range<u32>, out: &mut Vec<Delta>) {
        let tick = state.tick as u32;
        for site in sites {
            for &entity in index.at(site) {
                if below(rng.draw(tick, state.entities[entity as usize].id, DECISION)[0], PPM) < self.probability_ppm {
                    out.push(Delta::Remove { entity, recycle: self.recycle });
                }
            }
        }
    }
}
