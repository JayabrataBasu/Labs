//! `metabolism.genes`: entities act through their genes, then pay for existing.
//!
//! At each site, entities are served in a fresh random order every tick. An entity expresses
//! its genes in genome order; a gene is expressed if it senses nothing or if the site holds at
//! least `threshold` of the sensed resource (read before this rule changes anything):
//! - uptake: take up to `level` of a resource from the site;
//! - convert: turn up to `level` of one resource into another at `yield_num / yield_den`,
//!   but only if that reaction is listed in `reactions`. A share `leak_ppm` of the product
//!   escapes unavoidably: this "leaky function" is what lets others live off a producer
//!   (the Black Queen mechanism). The list is the world's chemistry:
//!   mutation may point a convert gene at any pair of resources, yet a pair the world does
//!   not allow does nothing (and still costs). Without this, one mutation could bypass a
//!   whole pathway and no dependence between entities could ever pay;
//! - secrete: release up to `level` of a resource to the site.
//!
//! Then the entity dissipates `base_cost + gene_cost * genes` of the energy resource, so
//! carrying a gene is never free. `release_to` says where secreted and leaked matter goes:
//! `"site"` (others can take it up) or `"sink"` (null world N1a: the producer's budget is
//! unchanged, but nothing is transferred to anyone).
//! `guests_trade_with` applies to contained entities (DR-022): `"host"` means a guest takes up
//! from, senses, secretes into and leaks into its host instead of the site; `"site"` means
//! containment only ties their positions together.
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
    /// The conversions this world allows.
    reactions: Vec<Reaction>,
    release_to: String,
    guests_trade_with: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Reaction {
    from: String,
    to: String,
    leak_ppm: u32,
}

pub struct Metabolism {
    stream: u16,
    energy: u16,
    base_cost: u64,
    gene_cost: u64,
    yield_num: u32,
    yield_den: u32,
    /// `leak[from * k + to]`: the leak of an allowed reaction, `None` if the world forbids it.
    leak: Vec<Option<u32>>,
    to_sink: bool,
    guests_inside: bool,
}

impl Metabolism {
    pub fn new(cfg: &RuleCfg, resources: &[String]) -> Result<Self, LabError> {
        let p: Params = params(cfg)?;
        if p.yield_den == 0 || p.yield_num > p.yield_den {
            return Err(invalid(cfg, "yield_num / yield_den must be between 0 and 1"));
        }
        let k = resources.len();
        let mut leak = vec![None; k * k];
        for reaction in &p.reactions {
            let (f, t) = (resource_index(cfg, &reaction.from, resources)? as usize, resource_index(cfg, &reaction.to, resources)? as usize);
            if f == t {
                return Err(invalid(cfg, "a reaction needs two different resources"));
            }
            if reaction.leak_ppm > 1_000_000 {
                return Err(invalid(cfg, "leak_ppm must be between 0 and 1000000"));
            }
            if leak[f * k + t].replace(reaction.leak_ppm).is_some() {
                return Err(invalid(cfg, "a reaction is listed twice"));
            }
        }
        let to_sink = match p.release_to.as_str() {
            "site" => false,
            "sink" => true,
            _ => return Err(invalid(cfg, "release_to must be \"site\" or \"sink\"")),
        };
        let guests_inside = match p.guests_trade_with.as_str() {
            "host" => true,
            "site" => false,
            _ => return Err(invalid(cfg, "guests_trade_with must be \"host\" or \"site\"")),
        };
        Ok(Metabolism {
            guests_inside,
            leak,
            stream: cfg.stream,
            energy: resource_index(cfg, &p.energy, resources)?,
            base_cost: p.base_cost,
            gene_cost: p.gene_cost,
            yield_num: p.yield_num,
            yield_den: p.yield_den,
            to_sink,
        })
    }
}

impl Rule for Metabolism {
    fn id(&self) -> &'static str {
        ID
    }
    fn version(&self) -> u32 {
        3
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
                // Where this entity's matter comes from and goes to: its site, or its host.
                let host = if self.guests_inside { state.host_of(entity as usize) } else { None };
                let (surroundings, stock) = match host {
                    Some(h) => (Holder::Entity(h as u32), &state.ent_stock[h * state.k..][..state.k]),
                    None => (Holder::Site(site), stock),
                };
                let release = if self.to_sink { Holder::Sink } else { surroundings };
                for (position, gene) in genome.iter().enumerate() {
                    if gene.sense != SENSE_ALWAYS && stock[gene.sense as usize] < u64::from(gene.threshold) {
                        continue;
                    }
                    let amount = u64::from(gene.level);
                    match gene.kind {
                        GeneKind::Uptake => {
                            out.push(Delta::Transfer { from: surroundings, to: Holder::Entity(entity), resource: u16::from(gene.a), amount })
                        }
                        GeneKind::Convert => {
                            let Some(leak_ppm) = self.leak[gene.a as usize * state.k + gene.b as usize] else {
                                continue;
                            };
                            let id = state.entities[entity as usize].id;
                            let leak_draw = if leak_ppm == 0 { 0 } else { rng.draw(tick, id, 1 + position as u16)[0] };
                            out.push(Delta::Convert {
                                entity,
                                from: u16::from(gene.a),
                                to: u16::from(gene.b),
                                amount,
                                yield_num: self.yield_num,
                                yield_den: self.yield_den,
                                leak_ppm,
                                leak_draw,
                                leak_to: release,
                            })
                        }
                        GeneKind::Secrete => {
                            out.push(Delta::Transfer { from: Holder::Entity(entity), to: release, resource: u16::from(gene.a), amount })
                        }
                        GeneKind::Bind => {} // acts through the containment rules
                    }
                }
                let cost = self.base_cost.saturating_add(self.gene_cost.saturating_mul(genome.len() as u64));
                out.push(Delta::Transfer { from: Holder::Entity(entity), to: Holder::Sink, resource: self.energy, amount: cost });
            }
        }
    }
}
