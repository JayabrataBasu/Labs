//! One level of containment between entities (DR-022). No benefit is built in: binding only ties
//! two bodies together, and what follows depends on the other rules and on genes, which cost.
//!
//! - `containment.bind`: free entities on a site that express a `bind` gene may pair up if their
//!   genes carry the same tag. Each eligible entity without guests tries, with a probability, to
//!   become the guest of another; hosts that already have guests are preferred, up to `max_guests`.
//!   Order is drawn afresh every tick.
//! - `containment.share`: for each listed resource, a share (`difference / divisor`) flows from the
//!   richer to the poorer of each guest and host.
//! - `containment.release`: each guest leaves with a probability.
//! - `containment.partition`: run it after reproduction. When a host has just divided, each of its
//!   guests moves with a probability: in mode `"lineage"` to the new daughter (vertical
//!   transmission); in mode `"random"` to any eligible free entity on the site, which breaks
//!   inheritance while keeping the amount of reshuffling (null world).
use crate::{invalid, params, resource_index};
use lab_core::config::RuleCfg;
use lab_core::{Delta, GeneKind, Holder, LabError, Rule, SiteIndex, State, SENSE_ALWAYS};
use lab_rng::{below, RuleRng};
use serde::Deserialize;
use std::ops::Range;

pub const BIND_ID: &str = "containment.bind";
pub const SHARE_ID: &str = "containment.share";
pub const RELEASE_ID: &str = "containment.release";
pub const PARTITION_ID: &str = "containment.partition";
const PPM: u32 = 1_000_000;

macro_rules! header {
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

/// Bit set of the tags of the bind genes an entity currently expresses at `site`.
fn tags(state: &State, entity: usize, site: u32) -> u32 {
    let stock = &state.site_stock[site as usize * state.k..][..state.k];
    state.genomes[entity]
        .iter()
        .filter(|g| g.kind == GeneKind::Bind && g.level > 0)
        .filter(|g| g.sense == SENSE_ALWAYS || stock[g.sense as usize] >= u64::from(g.threshold))
        .fold(0, |bits, g| bits | 1 << g.a)
}

/// Guests per resident of a site (indexed like `residents`).
fn guest_counts(state: &State, residents: &[u32]) -> Vec<u32> {
    let mut counts = vec![0u32; residents.len()];
    for parent in residents.iter().filter_map(|&e| state.entities[e as usize].parent) {
        if let Some(h) = residents.iter().position(|&e| state.entities[e as usize].id == parent) {
            counts[h] += 1;
        }
    }
    counts
}

// ---- containment.bind ---------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BindParams {
    probability_ppm: u32,
    max_guests: u32,
}

pub struct Bind {
    stream: u16,
    probability_ppm: u32,
    max_guests: u32,
}

impl Bind {
    pub fn new(cfg: &RuleCfg) -> Result<Self, LabError> {
        let p: BindParams = params(cfg)?;
        check_ppm(cfg, p.probability_ppm)?;
        Ok(Bind { stream: cfg.stream, probability_ppm: p.probability_ppm, max_guests: p.max_guests })
    }
}

impl Rule for Bind {
    header!(BIND_ID);
    fn propose(&self, state: &State, index: &SiteIndex, rng: RuleRng, sites: Range<u32>, out: &mut Vec<Delta>) {
        let tick = state.tick as u32;
        for site in sites {
            let residents = index.at(site);
            if residents.len() < 2 {
                continue;
            }
            let mut guests = guest_counts(state, residents);
            // (random key, position among residents, tags) of free entities that are willing.
            let mut willing: Vec<(u32, usize, u32)> = residents
                .iter()
                .enumerate()
                .filter(|(_, &e)| state.entities[e as usize].parent.is_none())
                .filter_map(|(i, &e)| {
                    let bits = tags(state, e as usize, site);
                    (bits != 0).then(|| (rng.draw(tick, state.entities[e as usize].id, 0)[0], i, bits))
                })
                .collect();
            willing.sort_unstable();
            let mut taken = vec![false; residents.len()]; // became a guest in this step
            for &(_, g, bits) in &willing {
                let id = state.entities[residents[g] as usize].id;
                if taken[g] || guests[g] > 0 || below(rng.draw(tick, id, 1)[0], PPM) >= self.probability_ppm {
                    continue;
                }
                // The most loaded host that still has room; ties go to the earliest in this tick's order.
                let host = willing
                    .iter()
                    .filter(|&&(_, h, other)| h != g && !taken[h] && other & bits != 0 && guests[h] < self.max_guests)
                    .rev()
                    .max_by_key(|&&(_, h, _)| guests[h]);
                if let Some(&(_, h, _)) = host {
                    taken[g] = true;
                    guests[h] += 1;
                    out.push(Delta::Attach { guest: residents[g], host: residents[h] });
                }
            }
        }
    }
}

// ---- containment.share --------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ShareParams {
    resources: Vec<String>,
    divisor: u64,
}

pub struct Share {
    stream: u16,
    resources: Vec<u16>,
    divisor: u64,
}

impl Share {
    pub fn new(cfg: &RuleCfg, resources: &[String]) -> Result<Self, LabError> {
        let p: ShareParams = params(cfg)?;
        if p.divisor == 0 || p.resources.is_empty() {
            return Err(invalid(cfg, "share needs at least one resource and a divisor of at least 1"));
        }
        let listed = p.resources.iter().map(|r| resource_index(cfg, r, resources)).collect::<Result<_, _>>()?;
        Ok(Share { stream: cfg.stream, resources: listed, divisor: p.divisor })
    }
}

impl Rule for Share {
    header!(SHARE_ID);
    fn propose(&self, state: &State, index: &SiteIndex, _: RuleRng, sites: Range<u32>, out: &mut Vec<Delta>) {
        for site in sites {
            for &guest in index.at(site) {
                let Some(host) = state.host_of(guest as usize) else { continue };
                for &r in &self.resources {
                    let mine = state.ent_stock[guest as usize * state.k + r as usize];
                    let theirs = state.ent_stock[host * state.k + r as usize];
                    let (from, to) = if mine > theirs { (guest, host as u32) } else { (host as u32, guest) };
                    let amount = mine.abs_diff(theirs) / self.divisor;
                    if amount > 0 {
                        out.push(Delta::Transfer { from: Holder::Entity(from), to: Holder::Entity(to), resource: r, amount });
                    }
                }
            }
        }
    }
}

// ---- containment.release ------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReleaseParams {
    probability_ppm: u32,
}

pub struct Release {
    stream: u16,
    probability_ppm: u32,
}

impl Release {
    pub fn new(cfg: &RuleCfg) -> Result<Self, LabError> {
        let p: ReleaseParams = params(cfg)?;
        check_ppm(cfg, p.probability_ppm)?;
        Ok(Release { stream: cfg.stream, probability_ppm: p.probability_ppm })
    }
}

impl Rule for Release {
    header!(RELEASE_ID);
    fn propose(&self, state: &State, index: &SiteIndex, rng: RuleRng, sites: Range<u32>, out: &mut Vec<Delta>) {
        let tick = state.tick as u32;
        for site in sites {
            for &guest in index.at(site) {
                let meta = &state.entities[guest as usize];
                if meta.parent.is_some() && below(rng.draw(tick, meta.id, 0)[0], PPM) < self.probability_ppm {
                    out.push(Delta::Detach { guest });
                }
            }
        }
    }
}

// ---- containment.partition ----------------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PartitionParams {
    probability_ppm: u32,
    mode: String,
    max_guests: u32,
}

pub struct Partition {
    stream: u16,
    probability_ppm: u32,
    random: bool,
    max_guests: u32,
}

impl Partition {
    pub fn new(cfg: &RuleCfg) -> Result<Self, LabError> {
        let p: PartitionParams = params(cfg)?;
        check_ppm(cfg, p.probability_ppm)?;
        let random = match p.mode.as_str() {
            "lineage" => false,
            "random" => true,
            _ => return Err(invalid(cfg, "mode must be \"lineage\" or \"random\"")),
        };
        Ok(Partition { stream: cfg.stream, probability_ppm: p.probability_ppm, random, max_guests: p.max_guests })
    }
}

impl Rule for Partition {
    header!(PARTITION_ID);
    fn propose(&self, state: &State, index: &SiteIndex, rng: RuleRng, sites: Range<u32>, out: &mut Vec<Delta>) {
        let tick = state.tick as u32;
        for site in sites {
            let residents = index.at(site);
            let meta = |i: usize| &state.entities[residents[i] as usize];
            let mut guests = guest_counts(state, residents);
            // Daughters born in this tick that are free, i.e. offspring of free mothers.
            let newborn: Vec<usize> = (0..residents.len()).filter(|&i| meta(i).born == state.tick && meta(i).mother.is_some() && meta(i).parent.is_none()).collect();
            if newborn.is_empty() {
                continue;
            }
            let mut gained = vec![false; residents.len()];
            for g in 0..residents.len() {
                let Some(host) = meta(g).parent else { continue };
                let Some(&daughter) = newborn.iter().find(|&&d| meta(d).mother == Some(host)) else { continue };
                let draw = rng.draw(tick, meta(g).id, 0);
                if below(draw[0], PPM) >= self.probability_ppm {
                    continue;
                }
                let target = if self.random {
                    let eligible: Vec<usize> = (0..residents.len())
                        .filter(|&i| i != g && meta(i).parent.is_none() && guests[i] < self.max_guests)
                        .filter(|&i| guests[i] > 0 || gained[i] || tags(state, residents[i] as usize, site) != 0)
                        .collect();
                    (!eligible.is_empty()).then(|| eligible[below(draw[1], eligible.len() as u32) as usize])
                } else {
                    (guests[daughter] < self.max_guests).then_some(daughter)
                };
                if let Some(t) = target {
                    guests[t] += 1;
                    gained[t] = true;
                    out.push(Delta::Attach { guest: residents[g], host: residents[t] });
                }
            }
        }
    }
}
