//! `influx.constant`: the outside world adds a fixed amount of one resource to every site.
use crate::{invalid, params, resource_index};
use lab_core::config::RuleCfg;
use lab_core::{Delta, Holder, LabError, Rule, SiteIndex, State};
use lab_rng::RuleRng;
use serde::Deserialize;
use std::ops::Range;

pub const ID: &str = "influx.constant";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Params {
    resource: String,
    amount: u64,
    /// Add on ticks divisible by this period.
    every: u64,
}

pub struct Influx {
    stream: u16,
    resource: u16,
    amount: u64,
    every: u64,
}

impl Influx {
    pub fn new(cfg: &RuleCfg, resources: &[String]) -> Result<Self, LabError> {
        let p: Params = params(cfg)?;
        if p.every == 0 {
            return Err(invalid(cfg, "every must be at least 1"));
        }
        Ok(Influx { stream: cfg.stream, resource: resource_index(cfg, &p.resource, resources)?, amount: p.amount, every: p.every })
    }
}

impl Rule for Influx {
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
        if !state.tick.is_multiple_of(self.every) {
            return;
        }
        for site in sites {
            out.push(Delta::Transfer { from: Holder::Source, to: Holder::Site(site), resource: self.resource, amount: self.amount });
        }
    }
}
