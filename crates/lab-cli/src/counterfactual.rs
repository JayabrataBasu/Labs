//! Counterfactual forks (DR-009): from one checkpoint, keep only some entities, run on under
//! the same rules and random keys, and report what the original members achieved.
//!
//! Removed entities' own stocks leave the system, but whatever they had already put on the
//! sites stays there, so a fork starts from the environment the whole community built.
//!
//! Random draws are keyed by entity id, so the original members face the same draws in every
//! fork. Descendants born during a fork receive fresh ids, so their draws differ between forks.
use crate::run::{write_text, Session};
use lab_core::{LabError, Observation};
use serde::Deserialize;
use serde_json::json;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    sets: Vec<Set>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Set {
    name: String,
    /// Ids to keep; `null` keeps everyone.
    keep: Option<Vec<u64>>,
    /// Ids to report on; they must be among those kept.
    report: Vec<u64>,
}

pub fn run(checkpoint: &Path, config: &Path, sets: &Path, ticks: u64, threads: usize, out: &Path) -> Result<usize, LabError> {
    let text = std::fs::read_to_string(sets).map_err(|e| LabError::Io(format!("cannot read {}: {e}", sets.display())))?;
    let request: Request = serde_json::from_str(&text).map_err(|e| LabError::Config(format!("{}: {e}", sets.display())))?;
    let mut results = Vec::new();
    for set in &request.sets {
        let mut session = Session::resumed(checkpoint, config, threads)?;
        if let Some(keep) = &set.keep {
            let mut keep = keep.clone();
            keep.sort_unstable();
            keep.dedup();
            let present = keep.iter().filter(|id| session.state.entities.binary_search_by_key(*id, |e| e.id).is_ok()).count();
            if present != keep.len() {
                return Err(LabError::Config(format!("set {:?}: some ids to keep are not alive in the checkpoint", set.name)));
            }
            session.state.retain_entities(&keep);
        }
        if let Some(missing) = set.report.iter().find(|id| session.state.entities.binary_search_by_key(*id, |e| e.id).is_err()) {
            return Err(LabError::Config(format!("set {:?}: entity {missing} to report on is not among those kept", set.name)));
        }
        let started = session.state.tick;
        let mut seen = Observation::default();
        for _ in 0..ticks {
            session.engine.step_observed(&mut session.state, &mut seen)?;
        }
        // Founder of each descendant: the original member whose lineage it belongs to.
        let mut lineage: BTreeMap<u64, u64> = set.report.iter().map(|&id| (id, id)).collect();
        let mut descendants: BTreeMap<u64, u64> = BTreeMap::new();
        let mut children: BTreeMap<u64, u64> = BTreeMap::new();
        for birth in &seen.births {
            if let Some(&root) = lineage.get(&birth.mother) {
                lineage.insert(birth.child, root);
                *descendants.entry(root).or_default() += 1;
                if root == birth.mother {
                    *children.entry(root).or_default() += 1;
                }
            }
        }
        let k = session.state.k;
        let mut produced: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
        for flow in seen.flows() {
            if let Some(&root) = lineage.get(&flow.entity) {
                produced.entry(root).or_insert_with(|| vec![0; k])[flow.resource as usize] += flow.produced;
            }
        }
        let members: Vec<_> = set
            .report
            .iter()
            .map(|&id| {
                let found = session.state.entities.binary_search_by_key(&id, |e| e.id).ok();
                json!({
                    "id": id,
                    "alive": found.is_some(),
                    "offspring": children.get(&id).copied().unwrap_or(0),
                    "descendants": descendants.get(&id).copied().unwrap_or(0),
                    "lineage_produced": produced.get(&id).cloned().unwrap_or_else(|| vec![0; k]),
                    "stocks": found.map(|i| session.state.ent_stock[i * k..(i + 1) * k].to_vec()),
                })
            })
            .collect();
        results.push(json!({
            "name": set.name, "start_tick": started, "end_tick": session.state.tick,
            "final_population": session.state.entities.len(), "members": members,
        }));
    }
    write_text(out, &format!("{}\n", json!({ "results": results })))?;
    Ok(results.len())
}
