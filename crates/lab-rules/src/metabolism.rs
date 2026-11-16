//! `metabolism.genes`: entities act through their genes, then pay for existing.
//!
//! At each site, entities are served in a fresh random order every tick. An entity expresses
//! its genes in genome order; a gene is expressed if it senses nothing or if the site holds at
//! least `threshold` of the sensed resource (read before this rule changes anything):
//! - uptake: take up to `level` of a resource from the site;
//! - convert: turn up to `level` of one resource into another at `yield_num / yield_den`,
//!   but only if that reaction is listed in `reactions`. The list is the world's chemistry:
//!   mutation may point a convert gene at any pair of resources, yet a pair the world does
//!   not allow does nothing (and still costs). Without this, one mutation could bypass a
//!   whole pathway and no dependence between entities could ever pay;
//! - secrete: release up to `level` of a resource to the site.
//!
//! Then the entity dissipates `base_cost + gene_cost * genes` of the energy resource, so
//! carrying a gene is never free. With `secretion = false` secrete genes are silent but still
//! cost: this is null world N1 (no interaction channel).
use crate::{invalid, params, resource_index};
use lab_core::config::RuleCfg;
use lab_core::{Delta, GeneKind, Holder, LabError, Rule, SiteIndex, State, SENSE_ALWAYS};
use lab_rng::RuleRng;
use serde::Deserialize;
use std::ops::Range;

pub const ID: &str = "metabolism.genes";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Params {
    energy: String,
    base_cost: u64,
    gene_cost: u64,
    yield_num: u32,
    yield_den: u32,
    /// Allowed conversions as `[from, to]` pairs of resource names.
    reactions: Vec<[String; 2]>,
    secretion: bool,
}

pub struct Metabolism {
    stream: u16,
    energy: u16,
    base_cost: u64,
    gene_cost: u64,
    yield_num: u32,
    yield_den: u32,
    /// `allowed[from * k + to]`
    allowed: Vec<bool>,
    secretion: bool,
}

impl Metabolism {
    pub fn new(cfg: &RuleCfg, resources: &[String]) -> Result<Self, LabError> {
        let p: Params = params(cfg)?;
        if p.yield_den == 0 || p.yield_num > p.yield_den {
            return Err(invalid(cfg, "yield_num / yield_den must be between 0 and 1"));
        }
        let k = resources.len();
        let mut allowed = vec![false; k * k];
        for [from, to] in &p.reactions {
            let (f, t) = (resource_index(cfg, from, resources)? as usize, resource_index(cfg, to, resources)? as usize);
            if f == t {
                return Err(invalid(cfg, "a reaction needs two different resources"));
            }
            allowed[f * k + t] = true;
        }
        Ok(Metabolism {
            allowed,
            stream: cfg.stream,
            energy: resource_index(cfg, &p.energy, resources)?,
            base_cost: p.base_cost,
            gene_cost: p.gene_cost,
            yield_num: p.yield_num,
            yield_den: p.yield_den,
            secretion: p.secretion,
        })
    }
}

impl Rule for Metabolism {
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
            let stock = &state.site_stock[site as usize * state.k..][..state.k];
            queue.clear();
            queue.extend(index.at(site).iter().map(|&e| {
                let id = state.entities[e as usize].id;
                (rng.draw(tick, id, 0)[0], id, e)
            }));
            queue.sort_unstable();
            for &(_, _, entity) in &queue {
                let genome = &state.genomes[entity as usize];
                for gene in genome {
                    if gene.sense != SENSE_ALWAYS && stock[gene.sense as usize] < u64::from(gene.threshold) {
                        continue;
                    }
                    let amount = u64::from(gene.level);
                    match gene.kind {
                        GeneKind::Uptake => {
                            out.push(Delta::Transfer { from: Holder::Site(site), to: Holder::Entity(entity), resource: u16::from(gene.a), amount })
                        }
                        GeneKind::Convert if !self.allowed[gene.a as usize * state.k + gene.b as usize] => {}
                        GeneKind::Convert => out.push(Delta::Convert {
                            entity,
                            from: u16::from(gene.a),
                            to: u16::from(gene.b),
                            amount,
                            yield_num: self.yield_num,
                            yield_den: self.yield_den,
                        }),
                        GeneKind::Secrete if self.secretion => {
                            out.push(Delta::Transfer { from: Holder::Entity(entity), to: Holder::Site(site), resource: u16::from(gene.a), amount })
                        }
                        GeneKind::Secrete => {}
                    }
                }
                let cost = self.base_cost.saturating_add(self.gene_cost.saturating_mul(genome.len() as u64));
                out.push(Delta::Transfer { from: Holder::Entity(entity), to: Holder::Sink, resource: self.energy, amount: cost });
            }
        }
    }
}
