//! A read-only record of what moved, who was born and who died (DR-021).
//!
//! Observation is filled by the apply step from the amounts that actually moved (after
//! clipping). It is not part of the state, is never read by a rule, and does not enter the
//! state hash, so watching a run cannot change it.

/// Totals for one entity, at one site, for one resource, since the observation was started.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Flow {
    pub entity: u64,
    pub site: u32,
    pub resource: u16,
    /// Moved from the site into the entity.
    pub taken: u64,
    /// Moved from the entity to the site: secretion, leakage, or its stocks at death.
    pub released: u64,
    /// Created inside the entity by conversion, including any share that leaked.
    pub produced: u64,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Birth {
    pub tick: u64,
    pub child: u64,
    pub mother: u64,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Death {
    pub tick: u64,
    pub id: u64,
}

/// Merge flows once this many unmerged records have piled up, to bound memory.
const COALESCE_AFTER: usize = 1 << 20;

#[derive(Default)]
pub struct Observation {
    flows: Vec<Flow>,
    /// Length of the merged prefix of `flows`.
    merged: usize,
    pub births: Vec<Birth>,
    pub deaths: Vec<Death>,
}

impl Observation {
    pub(crate) fn extend(&mut self, flows: &[Flow]) {
        self.flows.extend_from_slice(flows);
        if self.flows.len() - self.merged > COALESCE_AFTER.max(self.merged) {
            self.coalesce();
        }
    }

    fn coalesce(&mut self) {
        let key = |f: &Flow| (f.entity, f.site, f.resource);
        self.flows.sort_unstable_by_key(key);
        self.flows.dedup_by(|next, kept| {
            let same = key(next) == key(kept);
            if same {
                // Totals cannot overflow before the ledgers do; saturate rather than wrap regardless.
                kept.taken = kept.taken.saturating_add(next.taken);
                kept.released = kept.released.saturating_add(next.released);
                kept.produced = kept.produced.saturating_add(next.produced);
            }
            same
        });
        self.merged = self.flows.len();
    }

    /// One record per `(entity, site, resource)`, sorted by that key.
    pub fn flows(&mut self) -> &[Flow] {
        if self.merged != self.flows.len() {
            self.coalesce();
        }
        &self.flows
    }
}
