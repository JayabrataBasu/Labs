//! The M1 rule set and the registry that builds rules from a config.
//!
//! Each rule is one file. To add a mechanism: add a file, give it an id and version, and
//! register it in `build`. Nothing else in the kernel changes.
#![forbid(unsafe_code)]
#![deny(clippy::float_arithmetic)]

mod diffusion;
mod environment;
mod influx;
mod lifecycle;
mod maintenance;
mod metabolism;
pub mod mutation;
mod movement;
mod uptake;

use lab_core::config::{Config, RuleCfg};
use lab_core::{LabError, Rule};
use serde::de::DeserializeOwned;

/// Every rule id this build knows, for documentation and the world editor.
pub const RULE_IDS: &[&str] = &[
    influx::ID,
    diffusion::ID,
    environment::DECAY_ID,
    uptake::ID,
    maintenance::ID,
    metabolism::ID,
    movement::ID,
    environment::RELOCATE_ID,
    lifecycle::DIVISION_ID,
    lifecycle::RANDOM_BIRTH_ID,
    lifecycle::STARVATION_ID,
    lifecycle::RANDOM_DEATH_ID,
    environment::SHOCK_ID,
    environment::CULL_ID,
];

/// What the world editor offers when a rule is added: `(id, one-line description, params as JSON)`.
/// `$R0`, `$R1`, `$R2` stand for the world's first, second and third resource names.
pub const CATALOG: &[(&str, &str, &str)] = &[
    (influx::ID, "The outside world adds a resource to every site", r#"{"resource":"$R0","amount":2,"every":1}"#),
    (diffusion::ID, "Resource spreads to neighbouring sites", r#"{"resource":"$R0","divisor":8}"#),
    (environment::DECAY_ID, "A fraction of a resource at each site is lost", r#"{"resource":"$R0","divisor":20}"#),
    (uptake::ID, "Entities take a fixed amount from their site (no genes)", r#"{"resource":"$R0","amount":3}"#),
    (maintenance::ID, "Entities pay a fixed cost per tick (no genes)", r#"{"resource":"$R0","amount":1}"#),
    (
        metabolism::ID,
        "Entities act through their genes and pay for carrying them",
        r#"{"energy":"$R0","base_cost":2,"gene_cost":1,"yield_num":1,"yield_den":2,"reactions":[["$R1","$R2"]],"secretion":true}"#,
    ),
    (movement::ID, "Entities step to a random neighbouring site", r#"{"probability_ppm":100000}"#),
    (environment::RELOCATE_ID, "Entities jump to any site: removes spatial structure (null world N2)", r#"{"probability_ppm":100000}"#),
    (
        lifecycle::DIVISION_ID,
        "Entities holding enough of a resource divide; offspring may mutate",
        r#"{"resource":"$R0","threshold":200,"site_capacity":4,"mutation":{"point_ppm":20000,"point_magnitude":5,"retarget_ppm":2000,"duplicate_ppm":2000,"delete_ppm":5000,"max_genes":16}}"#,
    ),
    (
        lifecycle::RANDOM_BIRTH_ID,
        "Entities divide with a fixed probability whatever they hold (null world N3)",
        r#"{"probability_ppm":20000,"site_capacity":4,"mutation":{"point_ppm":0,"point_magnitude":0,"retarget_ppm":0,"duplicate_ppm":0,"delete_ppm":0,"max_genes":16}}"#,
    ),
    (lifecycle::STARVATION_ID, "Entities holding none of a resource die", r#"{"resource":"$R0"}"#),
    (lifecycle::RANDOM_DEATH_ID, "Entities die with a fixed probability (null world N3)", r#"{"probability_ppm":20000}"#),
    (
        environment::SHOCK_ID,
        "At one tick, add or remove a resource on a rectangle of sites",
        r#"{"tick":1000,"action":"remove","resource":"$R0","amount":100,"x":0,"y":0,"width":10,"height":10}"#,
    ),
    (environment::CULL_ID, "At one tick, remove each entity with a probability", r#"{"tick":1000,"probability_ppm":500000}"#),
];

/// Builds the configured rules, in config order.
pub fn build(cfg: &Config) -> Result<Vec<Box<dyn Rule>>, LabError> {
    let world = (cfg.world.width, cfg.world.height);
    cfg.rules.iter().map(|rule| build_one(rule, &cfg.world.resources, world)).collect()
}

fn build_one(cfg: &RuleCfg, resources: &[String], world: (u32, u32)) -> Result<Box<dyn Rule>, LabError> {
    let rule: Box<dyn Rule> = match cfg.id.as_str() {
        influx::ID => Box::new(influx::Influx::new(cfg, resources)?),
        diffusion::ID => Box::new(diffusion::Diffusion::new(cfg, resources)?),
        uptake::ID => Box::new(uptake::Uptake::new(cfg, resources)?),
        maintenance::ID => Box::new(maintenance::Maintenance::new(cfg, resources)?),
        movement::ID => Box::new(movement::Movement::new(cfg)?),
        metabolism::ID => Box::new(metabolism::Metabolism::new(cfg, resources)?),
        lifecycle::DIVISION_ID => Box::new(lifecycle::Division::new(cfg, resources)?),
        lifecycle::RANDOM_BIRTH_ID => Box::new(lifecycle::RandomBirth::new(cfg)?),
        lifecycle::STARVATION_ID => Box::new(lifecycle::Starvation::new(cfg, resources)?),
        lifecycle::RANDOM_DEATH_ID => Box::new(lifecycle::RandomDeath::new(cfg)?),
        environment::DECAY_ID => Box::new(environment::Decay::new(cfg, resources)?),
        environment::RELOCATE_ID => Box::new(environment::Relocate::new(cfg)?),
        environment::SHOCK_ID => Box::new(environment::Shock::new(cfg, resources, world)?),
        environment::CULL_ID => Box::new(environment::Cull::new(cfg)?),
        other => return Err(LabError::Config(format!("unknown rule id {other:?}"))),
    };
    if cfg.version != rule.version() {
        return Err(LabError::Config(format!(
            "rule {}: config asks for version {}, this build implements version {}",
            cfg.id,
            cfg.version,
            rule.version()
        )));
    }
    Ok(rule)
}

/// Deserializes a rule's parameter table; unknown and missing keys are errors.
fn params<P: DeserializeOwned>(cfg: &RuleCfg) -> Result<P, LabError> {
    toml::Value::Table(cfg.params.clone())
        .try_into()
        .map_err(|e| LabError::Config(format!("rule {} (stream {}): {e}", cfg.id, cfg.stream)))
}

fn resource_index(cfg: &RuleCfg, name: &str, resources: &[String]) -> Result<u16, LabError> {
    resources
        .iter()
        .position(|r| r == name)
        .map(|i| i as u16)
        .ok_or_else(|| LabError::Config(format!("rule {}: unknown resource {name:?}", cfg.id)))
}

fn invalid(cfg: &RuleCfg, message: &str) -> LabError {
    LabError::Config(format!("rule {} (stream {}): {message}", cfg.id, cfg.stream))
}
