//! `move.random`: with a fixed probability per tick, an entity steps to a uniformly chosen
//! neighbouring site. The probability is an integer number of parts per million.
use crate::{invalid, params};
use lab_core::config::RuleCfg;
use lab_core::{Delta, LabError, Rule, SiteIndex, State};
use lab_rng::{below, RuleRng};
use serde::Deserialize;
use std::ops::Range;

pub const ID: &str = "move.random";
const PPM: u32 = 1_000_000;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Params {
    probability_ppm: u32,
}

pub struct Movement {
    stream: u16,
    probability_ppm: u32,
}

impl Movement {
    pub fn new(cfg: &RuleCfg) -> Result<Self, LabError> {
        let p: Params = params(cfg)?;
        if p.probability_ppm > PPM {
            return Err(invalid(cfg, "probability_ppm must be between 0 and 1000000"));
        }
        Ok(Movement { stream: cfg.stream, probability_ppm: p.probability_ppm })
    }
}

impl Rule for Movement {
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
        for site in sites {
            let neighbours = state.topo.neighbours(site);
            if neighbours.is_empty() {
                continue;
            }
            for &entity in index.at(site) {
                if state.entities[entity as usize].parent.is_some() {
                    continue; // carried by its host
                }
                let draw = rng.draw(tick, state.entities[entity as usize].id, 0);
                if below(draw[0], PPM) < self.probability_ppm {
                    let to = neighbours[below(draw[1], neighbours.len() as u32) as usize];
                    out.push(Delta::Move { entity, to });
                }
            }
        }
    }
}
