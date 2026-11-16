//! Heritable variation at reproduction (DR-006).
//!
//! Four independent operators, each with its own rate in parts per million per gene, so that
//! mutation frequency, magnitude and structural change can be varied separately:
//! - `point`: a gene's `level` (or its `threshold`) changes by a uniform step in `-magnitude..=magnitude`;
//! - `retarget`: one of a gene's identities changes (resource, output resource, sensed resource, or kind);
//! - `duplicate`: the gene is copied (structural, lengthens the genome);
//! - `delete`: the gene is lost (structural, shortens the genome).
//!
//! Every draw is keyed by `(tick, parent id, gene position)`, so a mutation outcome is a pure
//! function of the seed and never depends on scheduling.
use lab_core::{Gene, GeneKind, LabError, SENSE_ALWAYS};
use lab_rng::{below, RuleRng};
use serde::Deserialize;

const PPM: u32 = 1_000_000;
/// Genome positions must fit in 15 bits of the RNG draw index.
pub const MAX_GENES_LIMIT: u32 = 4096;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MutationParams {
    pub point_ppm: u32,
    pub point_magnitude: u16,
    pub retarget_ppm: u32,
    pub duplicate_ppm: u32,
    pub delete_ppm: u32,
    /// Duplication is suppressed once a genome has this many genes.
    pub max_genes: u32,
}

impl MutationParams {
    pub fn validate(&self) -> Result<(), LabError> {
        let rates = [self.point_ppm, self.retarget_ppm, self.duplicate_ppm, self.delete_ppm];
        if rates.iter().any(|&r| r > PPM) {
            return Err(LabError::Config("mutation rates are in parts per million and must not exceed 1000000".into()));
        }
        if self.max_genes == 0 || self.max_genes > MAX_GENES_LIMIT {
            return Err(LabError::Config(format!("mutation.max_genes must be between 1 and {MAX_GENES_LIMIT}")));
        }
        Ok(())
    }

    /// The offspring genome of `parent`, born at `tick` to the entity with id `parent_id`. `k` is the number of resources.
    pub fn offspring(&self, parent: &[Gene], rng: RuleRng, tick: u32, parent_id: u64, k: u8) -> Vec<Gene> {
        let hit = |word: u32, rate: u32| below(word, PPM) < rate;
        let mut child = Vec::with_capacity(parent.len() + 1);
        for (i, &gene) in parent.iter().enumerate().take(MAX_GENES_LIMIT as usize) {
            let which = rng.draw(tick, parent_id, i as u16);
            let value = rng.draw(tick, parent_id, 0x8000 | i as u16);
            if hit(which[0], self.delete_ppm) {
                continue;
            }
            let mut g = gene;
            if hit(which[2], self.point_ppm) && self.point_magnitude > 0 {
                let span = 2 * u32::from(self.point_magnitude) + 1;
                let step = i64::from(below(value[0], span)) - i64::from(self.point_magnitude);
                let field = if value[1] & 1 == 0 || g.sense == SENSE_ALWAYS { &mut g.level } else { &mut g.threshold };
                *field = (i64::from(*field) + step).clamp(0, i64::from(u16::MAX)) as u16;
            }
            if hit(which[3], self.retarget_ppm) {
                retarget(&mut g, value[2], value[3], k);
            }
            child.push(g);
            // Duplication must leave room for the genes still to come, so the child never exceeds max_genes.
            let still_to_come = (parent.len() - i - 1) as u32;
            if hit(which[1], self.duplicate_ppm) && child.len() as u32 + still_to_come < self.max_genes {
                child.push(g);
            }
        }
        child
    }
}

/// Changes one identity of a gene, keeping it well-formed.
fn retarget(g: &mut Gene, choice: u32, value: u32, k: u8) {
    let resource = below(value, u32::from(k)) as u8;
    match below(choice, 4) {
        0 => g.a = resource,
        1 => g.b = resource,
        2 => {
            let sensed = below(value, u32::from(k) + 1) as u8;
            if sensed == k {
                g.sense = SENSE_ALWAYS;
                g.threshold = 0;
            } else {
                g.sense = sensed;
            }
        }
        _ => g.kind = GeneKind::ALL[below(value, 3) as usize],
    }
    if g.kind == GeneKind::Convert {
        if g.a == g.b {
            // A conversion needs two resources; with only one resource type it cannot exist.
            if k < 2 {
                g.kind = GeneKind::Uptake;
            } else {
                g.b = (g.a + 1) % k;
            }
        }
    } else {
        g.b = g.a;
    }
}
