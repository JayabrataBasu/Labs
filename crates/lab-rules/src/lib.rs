//! The M1 rule set and the registry that builds rules from a config.
//!
//! Each rule is one file. To add a mechanism: add a file, give it an id and version, and
//! register it in `build`. Nothing else in the kernel changes.
#![forbid(unsafe_code)]
#![deny(clippy::float_arithmetic)]

mod diffusion;
mod influx;
mod maintenance;
mod movement;
mod uptake;

use lab_core::config::{Config, RuleCfg};
use lab_core::{LabError, Rule};
use serde::de::DeserializeOwned;

/// Builds the configured rules, in config order.
pub fn build(cfg: &Config) -> Result<Vec<Box<dyn Rule>>, LabError> {
    cfg.rules.iter().map(|rule| build_one(rule, &cfg.world.resources)).collect()
}

fn build_one(cfg: &RuleCfg, resources: &[String]) -> Result<Box<dyn Rule>, LabError> {
    let rule: Box<dyn Rule> = match cfg.id.as_str() {
        influx::ID => Box::new(influx::Influx::new(cfg, resources)?),
        diffusion::ID => Box::new(diffusion::Diffusion::new(cfg, resources)?),
        uptake::ID => Box::new(uptake::Uptake::new(cfg, resources)?),
        maintenance::ID => Box::new(maintenance::Maintenance::new(cfg, resources)?),
        movement::ID => Box::new(movement::Movement::new(cfg)?),
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
