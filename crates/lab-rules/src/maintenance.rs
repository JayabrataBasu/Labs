//! `maintenance.fixed`: every entity dissipates a fixed amount of one resource per tick.
//! An entity holding less dissipates what it has; consequences of starvation arrive in M2.
use crate::{params, resource_index};
use lab_core::config::RuleCfg;
use lab_core::{Delta, Holder, LabError, Rule, SiteIndex, State};
use lab_rng::RuleRng;
use serde::Deserialize;
use std::ops::Range;

pub const ID: &str = "maintenance.fixed";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Params {
    resource: String,
    amount: u64,
}

pub struct Maintenance {
    stream: u16,
    resource: u16,
    amount: u64,
}

impl Maintenance {
    pub fn new(cfg: &RuleCfg, resources: &[String]) -> Result<Self, LabError> {
        let p: Params = params(cfg)?;
        Ok(Maintenance { stream: cfg.stream, resource: resource_index(cfg, &p.resource, resources)?, amount: p.amount })
    }
}

impl Rule for Maintenance {
    fn id(&self) -> &'static str {
        ID
    }
    fn version(&self) -> u32 {
        1
    }
    fn stream(&self) -> u16 {
        self.stream
    }
    fn propose(&self, _: &State, index: &SiteIndex, _: RuleRng, sites: Range<u32>, out: &mut Vec<Delta>) {
        for site in sites {
            for &entity in index.at(site) {
                out.push(Delta::Transfer { from: Holder::Entity(entity), to: Holder::Sink, resource: self.resource, amount: self.amount });
            }
        }
    }
}
