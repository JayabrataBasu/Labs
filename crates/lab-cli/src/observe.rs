//! Writing observation windows: the last `count` windows of `window` ticks of a run.
//!
//! Each window directory `obs/window-<start>-<end>/` holds the entities at its start and end,
//! the flows between entities and sites during it, and its births and deaths.
use crate::run::{entity_table, write_text};
use lab_core::{Engine, LabError, Observation, State};
use serde_json::json;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy)]
pub struct ObserveCfg {
    pub window: u64,
    pub count: u64,
}

pub struct Windows {
    cfg: ObserveCfg,
    /// First observed tick.
    from: u64,
    root: PathBuf,
    current: Option<(u64, PathBuf, Observation)>,
}

impl Windows {
    pub fn new(cfg: ObserveCfg, start: u64, end: u64, out: &Path) -> Result<Self, LabError> {
        let span = cfg.window.checked_mul(cfg.count).filter(|&span| cfg.window > 0 && cfg.count > 0 && span <= end - start);
        let Some(span) = span else {
            return Err(LabError::Config(format!(
                "--observe-window times --observe-count must be at least 1 and fit in the {} ticks of the run",
                end - start
            )));
        };
        Ok(Windows { cfg, from: end - span, root: out.join("obs"), current: None })
    }

    /// Advances one tick, observing it if it falls inside a window.
    pub fn step(&mut self, engine: &Engine, state: &mut State, resources: &[String]) -> Result<(), LabError> {
        if state.tick < self.from {
            return engine.step(state);
        }
        if self.current.is_none() {
            let (start, end) = (state.tick, state.tick + self.cfg.window);
            let dir = self.root.join(format!("window-{start:010}-{end:010}"));
            std::fs::create_dir_all(&dir).map_err(|e| LabError::Io(format!("cannot create {}: {e}", dir.display())))?;
            write_text(&dir.join("entities-start.csv"), &entity_table(state))?;
            self.current = Some((start, dir, Observation::default()));
        }
        let (start, dir, seen) = self.current.as_mut().expect("a window was just opened");
        engine.step_observed(state, seen)?;
        if state.tick == *start + self.cfg.window {
            write_text(&dir.join("entities-end.csv"), &entity_table(state))?;
            let mut flows = String::from("entity,site,resource,taken,released,produced\n");
            for f in seen.flows() {
                flows += &format!("{},{},{},{},{},{}\n", f.entity, f.site, f.resource, f.taken, f.released, f.produced);
            }
            write_text(&dir.join("flows.csv"), &flows)?;
            let births: String = seen.births.iter().map(|b| format!("{},{},{}\n", b.tick, b.child, b.mother)).collect();
            write_text(&dir.join("births.csv"), &format!("tick,child,mother\n{births}"))?;
            let deaths: String = seen.deaths.iter().map(|d| format!("{},{}\n", d.tick, d.id)).collect();
            write_text(&dir.join("deaths.csv"), &format!("tick,id\n{deaths}"))?;
            let meta = json!({
                "start": *start, "end": state.tick, "width": state.topo.width, "height": state.topo.n_sites() / state.topo.width,
                "wrap": state.topo.wrap, "resources": resources,
            });
            write_text(&dir.join("meta.json"), &format!("{meta}\n"))?;
            self.current = None;
        }
        Ok(())
    }
}
