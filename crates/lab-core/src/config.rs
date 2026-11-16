//! The declarative run configuration. Every field is required: there are no silent defaults.
use crate::{EntityMeta, LabError, State, Topology};
use lab_rng::{below, RuleRng, STREAM_INIT};
use serde::Deserialize;
use sha2::{Digest, Sha256};

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_RESOURCES: usize = 16;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema_version: u32,
    pub seed: u64,
    pub world: WorldCfg,
    pub init: InitCfg,
    pub rules: Vec<RuleCfg>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldCfg {
    pub width: u32,
    pub height: u32,
    pub wrap: bool,
    /// Resource names; their order defines resource indices.
    pub resources: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InitCfg {
    /// Initial stock of each resource at every site.
    pub site_stock: Vec<u64>,
    pub entities: u64,
    /// Initial stock of each resource in every entity.
    pub entity_stock: Vec<u64>,
}

#[derive(Clone, Debug, Deserialize)]
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
    Ok((cfg, Sha256::digest(text.as_bytes()).into()))
}

/// The tick-0 state. Entity `i` has id `i` and is placed by the reserved init stream.
pub fn initial_state(cfg: &Config) -> Result<State, LabError> {
    let topo = Topology::lattice(cfg.world.width, cfg.world.height, cfg.world.wrap)?;
    let n_sites = topo.n_sites();
    let k = cfg.world.resources.len();
    let n_entities = cfg.init.entities as usize;
    let site_stock = cfg.init.site_stock.repeat(n_sites as usize);
    let ent_stock = cfg.init.entity_stock.repeat(n_entities);
    let rng = RuleRng { seed: cfg.seed, stream: STREAM_INIT };
    let entities = (0..cfg.init.entities)
        .map(|id| EntityMeta { id, site: below(rng.draw(0, id, 0)[0], n_sites), parent: None })
        .collect();
    State::new(topo, k, site_stock, entities, ent_stock)
}
