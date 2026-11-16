//! The complete simulation state. Everything a run depends on is in here or in the config.
use crate::{LabError, Topology};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntityMeta {
    pub id: u64,
    pub site: u32,
    /// Containment slot reserved by DR-011. Always `None` until the fusion module (M5).
    pub parent: Option<u64>,
    /// Id of the entity this one was born from; `None` for founders. Lineage only, never read by rules.
    pub mother: Option<u64>,
    /// Tick during which the entity was created (0 for founders).
    pub born: u64,
}

/// What a gene does (DR-006). Stored as one byte in checkpoints.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum GeneKind {
    /// Take resource `a` from the site.
    Uptake = 0,
    /// Turn resource `a` into resource `b` inside the entity, with losses.
    Convert = 1,
    /// Release resource `a` to the site.
    Secrete = 2,
}

impl GeneKind {
    pub const ALL: [GeneKind; 3] = [GeneKind::Uptake, GeneKind::Convert, GeneKind::Secrete];
    pub fn from_u8(v: u8) -> Option<Self> {
        Self::ALL.get(v as usize).copied()
    }
    pub fn name(self) -> &'static str {
        ["uptake", "convert", "secrete"][self as usize]
    }
}

/// `sense` value meaning "always expressed".
pub const SENSE_ALWAYS: u8 = 255;

/// One heritable unit of capability. A genome is an ordered, variable-length list of genes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gene {
    pub kind: GeneKind,
    /// Resource acted on (the input, for `Convert`).
    pub a: u8,
    /// Output resource of `Convert`; equal to `a` for other kinds.
    pub b: u8,
    /// Resource whose amount at the entity's site gates expression, or `SENSE_ALWAYS`.
    pub sense: u8,
    /// Units per tick.
    pub level: u16,
    /// Expressed only if the site holds at least this much of `sense`.
    pub threshold: u16,
}

/// Cumulative flows per resource: across the system boundary and through conversion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ledger {
    pub influx: Vec<u64>,
    pub dissipated: Vec<u64>,
    /// Units of each resource produced by conversion.
    pub converted_in: Vec<u64>,
    /// Units of each resource consumed by conversion.
    pub converted_out: Vec<u64>,
}

impl Ledger {
    pub fn zero(k: usize) -> Self {
        Ledger { influx: vec![0; k], dissipated: vec![0; k], converted_in: vec![0; k], converted_out: vec![0; k] }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct State {
    pub tick: u64,
    /// Number of resource types.
    pub k: usize,
    pub topo: Topology,
    /// `n_sites * k` values; resource `r` at site `s` is `site_stock[s * k + r]`.
    pub site_stock: Vec<u64>,
    /// Always sorted by strictly ascending id.
    pub entities: Vec<EntityMeta>,
    /// `entities.len() * k` values, aligned with `entities`.
    pub ent_stock: Vec<u64>,
    /// One genome per entity, aligned with `entities`. Empty genomes are allowed.
    pub genomes: Vec<Vec<Gene>>,
    pub ledger: Ledger,
    pub next_entity_id: u64,
    /// Cumulative counts of entities created after tick 0 and of entities removed.
    pub births: u64,
    pub deaths: u64,
    /// Per-resource total at tick 0, the anchor of the conservation identity.
    pub initial_total: Vec<u64>,
}

impl State {
    /// Builds a tick-0 state of entities without genes and validates its internal consistency.
    pub fn new(
        topo: Topology,
        k: usize,
        site_stock: Vec<u64>,
        entities: Vec<EntityMeta>,
        ent_stock: Vec<u64>,
    ) -> Result<Self, LabError> {
        let genomes = vec![Vec::new(); entities.len()];
        Self::with_genomes(topo, k, site_stock, entities, ent_stock, genomes)
    }

    /// Builds a tick-0 state and validates its internal consistency.
    pub fn with_genomes(
        topo: Topology,
        k: usize,
        site_stock: Vec<u64>,
        entities: Vec<EntityMeta>,
        ent_stock: Vec<u64>,
        genomes: Vec<Vec<Gene>>,
    ) -> Result<Self, LabError> {
        let next_entity_id = match entities.last() {
            Some(e) => e.id.checked_add(1).ok_or(LabError::Overflow("entity id"))?,
            None => 0,
        };
        let mut state = State {
            tick: 0,
            k,
            topo,
            site_stock,
            entities,
            ent_stock,
            genomes,
            ledger: Ledger::zero(k),
            next_entity_id,
            births: 0,
            deaths: 0,
            initial_total: Vec::new(),
        };
        state.validate().map_err(LabError::Config)?;
        state.initial_total = (0..k).map(|r| state.total(r)).collect::<Result<_, _>>()?;
        Ok(state)
    }

    /// Structural invariants shared by construction and checkpoint loading.
    pub(crate) fn validate(&self) -> Result<(), String> {
        let n = self.topo.n_sites() as usize;
        if self.k == 0 {
            return Err("at least one resource type is required".into());
        }
        if n.checked_mul(self.k) != Some(self.site_stock.len()) {
            return Err(format!("site_stock has {} values, expected {} sites x {} resources", self.site_stock.len(), n, self.k));
        }
        if self.entities.len().checked_mul(self.k) != Some(self.ent_stock.len()) {
            return Err(format!("ent_stock has {} values, expected {} entities x {} resources", self.ent_stock.len(), self.entities.len(), self.k));
        }
        if self.entities.len() > u32::MAX as usize {
            return Err("too many entities".into());
        }
        let ledgers = [&self.ledger.influx, &self.ledger.dissipated, &self.ledger.converted_in, &self.ledger.converted_out];
        if ledgers.iter().any(|l| l.len() != self.k) {
            return Err("ledger length does not match the number of resources".into());
        }
        if self.genomes.len() != self.entities.len() {
            return Err(format!("{} genomes for {} entities", self.genomes.len(), self.entities.len()));
        }
        let resource_ok = |r: u8| (r as usize) < self.k;
        if self.genomes.iter().flatten().any(|g| !resource_ok(g.a) || !resource_ok(g.b) || (g.sense != SENSE_ALWAYS && !resource_ok(g.sense))) {
            return Err("a gene refers to a resource that does not exist".into());
        }
        if self.entities.windows(2).any(|w| w[0].id >= w[1].id) {
            return Err("entities must be sorted by strictly ascending id".into());
        }
        if let Some(e) = self.entities.iter().find(|e| e.site as usize >= n) {
            return Err(format!("entity {} is at site {}, but there are only {} sites", e.id, e.site, n));
        }
        if let Some(e) = self.entities.last() {
            if e.id >= self.next_entity_id {
                return Err("next_entity_id must exceed every existing id".into());
            }
        }
        Ok(())
    }

    /// Total of resource `r` held by all sites and entities.
    pub fn total(&self, r: usize) -> Result<u64, LabError> {
        let mut sum = 0u64;
        for chunk in self.site_stock.chunks_exact(self.k).chain(self.ent_stock.chunks_exact(self.k)) {
            sum = sum.checked_add(chunk[r]).ok_or(LabError::Overflow("resource total"))?;
        }
        Ok(sum)
    }

    /// Groups entity indices by site, ascending id within a site.
    pub fn site_index(&self) -> SiteIndex {
        let n = self.topo.n_sites() as usize;
        let mut offsets = vec![0u32; n + 1];
        for e in &self.entities {
            offsets[e.site as usize + 1] += 1;
        }
        for s in 0..n {
            offsets[s + 1] += offsets[s];
        }
        let mut cursor = offsets.clone();
        let mut items = vec![0u32; self.entities.len()];
        for (i, e) in self.entities.iter().enumerate() {
            let c = &mut cursor[e.site as usize];
            items[*c as usize] = i as u32;
            *c += 1;
        }
        let mut position = vec![0u32; items.len()];
        for (pos, &entity) in items.iter().enumerate() {
            position[entity as usize] = pos as u32;
        }
        SiteIndex { offsets, items, position }
    }
}

/// Which entities are at each site, valid for the state it was built from.
#[derive(Clone, Debug)]
pub struct SiteIndex {
    offsets: Vec<u32>,
    items: Vec<u32>,
    position: Vec<u32>,
}

impl SiteIndex {
    /// All entity indices ordered by site, then id.
    pub fn items(&self) -> &[u32] {
        &self.items
    }

    /// Where entity index `entity` sits in `items()`.
    pub fn position(&self, entity: u32) -> usize {
        self.position[entity as usize] as usize
    }

    /// The part of `items()` occupied by entities at the given contiguous sites.
    pub fn span(&self, sites: std::ops::Range<u32>) -> std::ops::Range<usize> {
        self.offsets[sites.start as usize] as usize..self.offsets[sites.end as usize] as usize
    }

    /// Indices into `State::entities` of the entities at `site`, ascending id.
    pub fn at(&self, site: u32) -> &[u32] {
        let s = site as usize;
        &self.items[self.offsets[s] as usize..self.offsets[s + 1] as usize]
    }
}
