//! `diffusion.integer`: each site sends `stock / divisor` of one resource to each neighbour.
//!
//! Amounts come from the state before this rule runs. With at most 4 neighbours and
//! `divisor >= 5` a site never sends more than it holds, so no transfer is clipped and the
//! result does not depend on the order of sites.
use crate::{invalid, params, resource_index};
use lab_core::config::RuleCfg;
use lab_core::{Delta, Holder, LabError, Rule, SiteIndex, State};
use lab_rng::RuleRng;
use serde::Deserialize;
use std::ops::Range;

pub const ID: &str = "diffusion.integer";
const MIN_DIVISOR: u64 = 5;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Params {
    resource: String,
    divisor: u64,
}

pub struct Diffusion {
    stream: u16,
    resource: u16,
    divisor: u64,
}

impl Diffusion {
    pub fn new(cfg: &RuleCfg, resources: &[String]) -> Result<Self, LabError> {
        let p: Params = params(cfg)?;
        if p.divisor < MIN_DIVISOR {
            return Err(invalid(cfg, "divisor must be at least 5 (more than the number of neighbours)"));
        }
        Ok(Diffusion { stream: cfg.stream, resource: resource_index(cfg, &p.resource, resources)?, divisor: p.divisor })
    }
}

impl Rule for Diffusion {
    fn id(&self) -> &'static str {
        ID
    }
    fn version(&self) -> u32 {
        1
    }
    fn stream(&self) -> u16 {
        self.stream
    }
    fn propose(&self, state: &State, _: &SiteIndex, _: RuleRng, sites: Range<u32>, out: &mut Vec<Delta>) {
        for site in sites {
            let share = state.site_stock[site as usize * state.k + self.resource as usize] / self.divisor;
            if share == 0 {
                continue;
            }
            for &neighbour in state.topo.neighbours(site) {
                out.push(Delta::Transfer { from: Holder::Site(site), to: Holder::Site(neighbour), resource: self.resource, amount: share });
            }
        }
    }
}
