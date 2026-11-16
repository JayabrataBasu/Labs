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
