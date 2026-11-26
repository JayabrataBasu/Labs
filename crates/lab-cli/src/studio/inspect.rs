//! Read-only views of the published state: map layers, metabolic types, sites and entities.
use super::param;
use super::session::LiveRun;
use crate::edit::{error, header, json_response, Reply};
use lab_core::{Gene, GeneKind, State, SENSE_ALWAYS};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::sync::Arc;
use tiny_http::Response;

pub fn get(live: &LiveRun, path: &str, query: &str) -> Result<Reply, Reply> {
    let shared = &live.shared;
    let state = Arc::clone(&shared.snapshot.lock().expect("snapshot lock"));
    let names = &shared.resources;
    match path {
        "/history" => {
            let history = shared.history.lock().expect("history lock");
            let column = |f: &dyn Fn(&super::session::Row) -> u64| history.rows.iter().map(f).collect::<Vec<_>>();
            Ok(json_response(
                200,
                json!({
                    "tick": column(&|r| r.tick),
                    "entities": column(&|r| r.entities),
                    "contained": column(&|r| r.contained),
                    "genes": {
                        "uptake": column(&|r| r.genes[0]), "convert": column(&|r| r.genes[1]),
                        "secrete": column(&|r| r.genes[2]), "bind": column(&|r| r.genes[3]),
                    },
                    "site": (0..names.len()).map(|i| column(&|r| r.site[i])).collect::<Vec<_>>(),
                }),
            ))
        }
        "/layer" => {
            let n = state.topo.n_sites() as usize;
            let mut values = vec![0u32; n];
            match param(query, "kind") {
                Some("stock") => {
                    let r = param(query, "resource").and_then(|v| v.parse::<usize>().ok()).filter(|&r| r < state.k);
                    let r = r.ok_or_else(|| error(400, "resource must be a valid resource index"))?;
                    for (v, stocks) in values.iter_mut().zip(state.site_stock.chunks_exact(state.k)) {
                        *v = u32::try_from(stocks[r]).unwrap_or(u32::MAX);
                    }
                }
                Some("count") => state.entities.iter().for_each(|e| values[e.site as usize] += 1),
                Some("guests") => state.entities.iter().filter(|e| e.parent.is_some()).for_each(|e| values[e.site as usize] += 1),
                Some("type") => {
                    // The most common type at each site; ties go to the type seen first in the run.
                    let codes = type_codes(&mut shared.types.lock().expect("types lock"), &state, names);
                    let mut here: BTreeMap<(u32, u32), u32> = BTreeMap::new();
                    for (e, &code) in state.entities.iter().zip(&codes) {
                        *here.entry((e.site, code)).or_default() += 1;
                    }
                    let mut best = vec![0u32; n];
                    for ((site, code), count) in here {
                        if count > best[site as usize] {
                            (best[site as usize], values[site as usize]) = (count, code);
                        }
                    }
                }
                _ => return Err(error(400, "kind must be stock, count, guests or type")),
            }
            let mut bytes = Vec::with_capacity(8 + 4 * n);
            bytes.extend_from_slice(&state.tick.to_le_bytes());
            values.iter().for_each(|v| bytes.extend_from_slice(&v.to_le_bytes()));
            Ok(Response::from_data(bytes).with_header(header("Content-Type", "application/octet-stream")))
        }
        "/types" => {
            let mut registry = shared.types.lock().expect("types lock");
            let codes = type_codes(&mut registry, &state, names);
            let mut counts = vec![0u64; registry.len()];
            codes.iter().for_each(|&c| counts[c as usize - 1] += 1);
            let mut present: Vec<(u64, usize)> = counts.iter().enumerate().filter(|(_, &n)| n > 0).map(|(i, &n)| (n, i)).collect();
            present.sort_unstable_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
            let list: Vec<Value> = present.iter().map(|&(count, i)| json!({ "code": i + 1, "label": registry[i], "count": count })).collect();
            Ok(json_response(200, Value::Array(list)))
        }
        "/site" => {
            let coordinate = |key: &str, limit: u32| param(query, key).and_then(|v| v.parse::<u32>().ok()).filter(|&v| v < limit);
            let (Some(x), Some(y)) = (coordinate("x", shared.width), coordinate("y", shared.height)) else {
                return Err(error(400, "x and y must lie inside the world"));
            };
            let site = y * shared.width + x;
            let stocks = named_stocks(&state.site_stock[site as usize * state.k..][..state.k], names);
            let entities: Vec<Value> = (0..state.entities.len()).filter(|&i| state.entities[i].site == site).map(|i| entity(&state, i, names)).collect();
            Ok(json_response(200, json!({ "x": x, "y": y, "stocks": stocks, "entities": entities })))
        }
        "/entity" => {
            let index = param(query, "id").and_then(|v| v.parse::<u64>().ok()).and_then(|id| state.index_of(id));
            let index = index.ok_or_else(|| error(404, "no living entity has that id"))?;
            Ok(json_response(200, entity(&state, index, names)))
        }
        _ => unreachable!("routed paths only"),
    }
}

/// A metabolic type: which conversions a genome can run. Descriptive only; nothing depends on it.
fn type_label(genome: &[Gene], names: &[String]) -> String {
    let mut pairs: Vec<(u8, u8)> = genome.iter().filter(|g| g.kind == GeneKind::Convert && g.level > 0).map(|g| (g.a, g.b)).collect();
    pairs.sort_unstable();
    pairs.dedup();
    if pairs.is_empty() {
        return "no conversions".into();
    }
    pairs.iter().map(|&(a, b)| format!("{}→{}", names[a as usize], names[b as usize])).collect::<Vec<_>>().join(", ")
}

/// The type code of every entity, registering labels not seen before.
fn type_codes(registry: &mut Vec<String>, state: &State, names: &[String]) -> Vec<u32> {
    let mut known: BTreeMap<String, u32> = registry.iter().enumerate().map(|(i, l)| (l.clone(), i as u32 + 1)).collect();
    state
        .genomes
        .iter()
        .map(|genome| {
            let label = type_label(genome, names);
            *known.entry(label).or_insert_with_key(|label| {
                registry.push(label.clone());
                registry.len() as u32
            })
        })
        .collect()
}

fn named_stocks(stocks: &[u64], names: &[String]) -> Value {
    Value::Object(names.iter().zip(stocks).map(|(n, &v)| (n.clone(), v.into())).collect::<Map<_, _>>())
}

fn gene_text(g: &Gene, names: &[String]) -> String {
    let (a, b) = (&names[g.a as usize], &names[g.b as usize]);
    let what = match g.kind {
        GeneKind::Convert => format!("convert {a}→{b}"),
        kind => format!("{} {a}", kind.name()),
    };
    let gate = if g.sense == SENSE_ALWAYS { String::new() } else { format!(" if {} ≥ {}", names[g.sense as usize], g.threshold) };
    format!("{what} level {}{gate}", g.level)
}

fn entity(state: &State, index: usize, names: &[String]) -> Value {
    let e = &state.entities[index];
    let ids = |keep: &dyn Fn(&lab_core::EntityMeta) -> bool| state.entities.iter().filter(|&o| keep(o)).map(|o| o.id).collect::<Vec<_>>();
    json!({
        "id": e.id,
        "site": e.site,
        "born": e.born,
        "mother": e.mother,
        "mother_alive": e.mother.is_some_and(|m| state.index_of(m).is_some()),
        "offspring_alive": ids(&|o| o.mother == Some(e.id)),
        "host": e.parent,
        "guests": ids(&|o| o.parent == Some(e.id)),
        "stocks": named_stocks(&state.ent_stock[index * state.k..][..state.k], names),
        "type": type_label(&state.genomes[index], names),
        "genes": state.genomes[index].iter().map(|g| gene_text(g, names)).collect::<Vec<_>>(),
    })
}
