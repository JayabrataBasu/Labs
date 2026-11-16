//! The declarative run configuration. Every field is required: there are no silent defaults.
use crate::{EntityMeta, Gene, GeneKind, LabError, State, Topology, SENSE_ALWAYS};
use lab_rng::{below, RuleRng, STREAM_INIT};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_RESOURCES: usize = 16;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema_version: u32,
    pub seed: u64,
    pub world: WorldCfg,
    pub init: InitCfg,
    pub rules: Vec<RuleCfg>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorldCfg {
    pub width: u32,
    pub height: u32,
    pub wrap: bool,
    /// Resource names; their order defines resource indices.
    pub resources: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InitCfg {
    /// Initial stock of each resource at every site.
    pub site_stock: Vec<u64>,
    /// Rectangles of extra resource added on top of `site_stock`; may be empty.
    pub patches: Vec<PatchCfg>,
    pub entities: u64,
    /// Initial stock of each resource in every entity.
    pub entity_stock: Vec<u64>,
    /// The genome every founder starts with; may be empty.
    pub genome: Vec<GeneCfg>,
}

/// Adds `amount` of `resource` to every site of a rectangle that lies inside the lattice.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PatchCfg {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub resource: String,
    pub amount: u64,
}

/// A gene as written in a config. `sense` is a resource name or `"always"` (then `threshold` must be 0).
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum GeneCfg {
    Uptake { resource: String, level: u16, sense: String, threshold: u16 },
    Convert { from: String, to: String, level: u16, sense: String, threshold: u16 },
    Secrete { resource: String, level: u16, sense: String, threshold: u16 },
}

/// Index of a named resource.
pub fn resource_index(name: &str, resources: &[String]) -> Result<u8, LabError> {
    resources.iter().position(|r| r == name).map(|i| i as u8).ok_or_else(|| LabError::Config(format!("unknown resource {name:?}")))
}

/// Turns configured genes into kernel genes.
pub fn resolve_genome(genes: &[GeneCfg], resources: &[String]) -> Result<Vec<Gene>, LabError> {
    genes
        .iter()
        .map(|gene| {
            let (kind, a, b, level, sense, threshold) = match gene {
                GeneCfg::Uptake { resource, level, sense, threshold } => (GeneKind::Uptake, resource, resource, level, sense, threshold),
                GeneCfg::Convert { from, to, level, sense, threshold } => (GeneKind::Convert, from, to, level, sense, threshold),
                GeneCfg::Secrete { resource, level, sense, threshold } => (GeneKind::Secrete, resource, resource, level, sense, threshold),
            };
            let (a, b) = (resource_index(a, resources)?, resource_index(b, resources)?);
            if kind == GeneKind::Convert && a == b {
                return Err(LabError::Config("a convert gene needs two different resources".into()));
            }
            let sense = if sense == "always" {
                if *threshold != 0 {
                    return Err(LabError::Config("a gene with sense = \"always\" must have threshold = 0".into()));
                }
                SENSE_ALWAYS
            } else {
                resource_index(sense, resources)?
            };
            Ok(Gene { kind, a, b, sense, level: *level, threshold: *threshold })
        })
        .collect()
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RuleCfg {
    pub id: String,
    pub version: u32,
    pub stream: u16,
    pub params: toml::Table,
}

/// Parses and validates a config. Also returns the SHA-256 of the exact text, which
/// identifies the configuration in checkpoints and manifests.
pub fn parse(text: &str) -> Result<(Config, [u8; 32]), LabError> {
    let cfg: Config = toml::from_str(text).map_err(|e| LabError::Config(e.to_string()))?;
    let bad = |m: String| Err(LabError::Config(m));
    if cfg.schema_version != SCHEMA_VERSION {
        return bad(format!("schema_version is {}, this build reads {SCHEMA_VERSION}", cfg.schema_version));
    }
    let k = cfg.world.resources.len();
    if k == 0 || k > MAX_RESOURCES {
        return bad(format!("world.resources must name between 1 and {MAX_RESOURCES} resources"));
    }
    for (i, name) in cfg.world.resources.iter().enumerate() {
        if name.is_empty() || cfg.world.resources[..i].contains(name) {
            return bad(format!("world.resources: duplicate or empty resource name {name:?}"));
        }
    }
    if cfg.init.site_stock.len() != k {
        return bad(format!("init.site_stock has {} values but there are {k} resources", cfg.init.site_stock.len()));
    }
    if cfg.init.entity_stock.len() != k {
        return bad(format!("init.entity_stock has {} values but there are {k} resources", cfg.init.entity_stock.len()));
    }
    if cfg.init.entities > u64::from(u32::MAX) {
        return bad("init.entities is too large".into());
    }
    for (i, rule) in cfg.rules.iter().enumerate() {
        if rule.stream == STREAM_INIT {
            return bad(format!("rule {}: stream {STREAM_INIT} is reserved for initial placement", rule.id));
        }
        if cfg.rules[..i].iter().any(|r| r.stream == rule.stream) {
            return bad(format!("rule {}: stream {} is already used by another rule", rule.id, rule.stream));
        }
    }
    Topology::lattice(cfg.world.width, cfg.world.height, cfg.world.wrap)?;
    resolve_genome(&cfg.init.genome, &cfg.world.resources).map_err(|e| LabError::Config(format!("init.genome: {e}")))?;
    for (i, patch) in cfg.init.patches.iter().enumerate() {
        resource_index(&patch.resource, &cfg.world.resources).map_err(|e| LabError::Config(format!("init.patches[{i}]: {e}")))?;
        let fits = |start: u32, len: u32, limit: u32| len > 0 && start.checked_add(len).is_some_and(|end| end <= limit);
        if !fits(patch.x, patch.width, cfg.world.width) || !fits(patch.y, patch.height, cfg.world.height) {
            return bad(format!("init.patches[{i}] does not lie inside the {}x{} lattice", cfg.world.width, cfg.world.height));
        }
    }
    Ok((cfg, Sha256::digest(text.as_bytes()).into()))
}

/// The tick-0 state. Entity `i` has id `i` and is placed by the reserved init stream.
pub fn initial_state(cfg: &Config) -> Result<State, LabError> {
    let topo = Topology::lattice(cfg.world.width, cfg.world.height, cfg.world.wrap)?;
    let n_sites = topo.n_sites();
    let k = cfg.world.resources.len();
    let n_entities = cfg.init.entities as usize;
    let mut site_stock = cfg.init.site_stock.repeat(n_sites as usize);
    for patch in &cfg.init.patches {
        let r = resource_index(&patch.resource, &cfg.world.resources)? as usize;
        for y in patch.y..patch.y + patch.height {
            for x in patch.x..patch.x + patch.width {
                let stock = &mut site_stock[(y * cfg.world.width + x) as usize * k + r];
                *stock = stock.checked_add(patch.amount).ok_or(LabError::Overflow("initial site stock"))?;
            }
        }
    }
    let genome = resolve_genome(&cfg.init.genome, &cfg.world.resources)?;
    let ent_stock = cfg.init.entity_stock.repeat(n_entities);
    let rng = RuleRng { seed: cfg.seed, stream: STREAM_INIT };
    let entities = (0..cfg.init.entities)
        .map(|id| EntityMeta { id, site: below(rng.draw(0, id, 0)[0], n_sites), parent: None, mother: None, born: 0 })
        .collect();
    State::with_genomes(topo, k, site_stock, entities, ent_stock, vec![genome; n_entities])
}
