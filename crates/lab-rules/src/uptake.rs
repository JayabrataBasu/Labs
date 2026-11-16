//! `uptake.fixed`: every entity requests a fixed amount of one resource from its site.
//!
//! When a site cannot satisfy everyone, requests are served in a fresh random order each
//! tick. Serving in id order would systematically favour older entities.
use crate::{params, resource_index};
use lab_core::config::RuleCfg;
use lab_core::{Delta, Holder, LabError, Rule, SiteIndex, State};
use lab_rng::RuleRng;
use serde::Deserialize;
use std::ops::Range;

pub const ID: &str = "uptake.fixed";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Params {
    resource: String,
    amount: u64,
}

pub struct Uptake {
    stream: u16,
    resource: u16,
    amount: u64,
}

impl Uptake {
    pub fn new(cfg: &RuleCfg, resources: &[String]) -> Result<Self, LabError> {
        let p: Params = params(cfg)?;
        Ok(Uptake { stream: cfg.stream, resource: resource_index(cfg, &p.resource, resources)?, amount: p.amount })
    }
}

impl Rule for Uptake {
    fn id(&self) -> &'static str {
        ID
    }
    fn version(&self) -> u32 {
        1
    }
    fn stream(&self) -> u16 {
        self.stream
    }
    fn propose(&self, state: &State, index: &SiteIndex, rng: RuleRng, sites: Range<u32>, out: &mut Vec<Delta>) {
        let tick = state.tick as u32;
        let mut queue: Vec<(u32, u64, u32)> = Vec::new();
        for site in sites {
            queue.clear();
            queue.extend(index.at(site).iter().map(|&e| {
                let id = state.entities[e as usize].id;
                (rng.draw(tick, id, 0)[0], id, e)
            }));
            queue.sort_unstable();
            for &(_, _, entity) in &queue {
                out.push(Delta::Transfer { from: Holder::Site(site), to: Holder::Entity(entity), resource: self.resource, amount: self.amount });
            }
        }
    }
}
