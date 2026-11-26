//! A live run: the simulation on its own thread, publishing copies of its state for the page.
//! It is an ordinary run with the ordinary outputs; the copies are all the page ever sees.
use super::{named_toml, Studio};
use crate::edit::{error, json_response, Reply};
use crate::live::Gate;
use crate::run::{execute, hex, Outputs, Session};
use lab_core::State;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// How often a running simulation hands a fresh copy to the page.
const PUBLISH_EVERY: Duration = Duration::from_millis(100);
/// History keeps between half this many and this many points, thinning as a run grows.
const HISTORY_POINTS: usize = 800;

pub struct Row {
    pub tick: u64,
    pub entities: u64,
    pub contained: u64,
    pub genes: [u64; 4],
    pub site: Vec<u64>,
}

impl Row {
    fn of(state: &State) -> Self {
        let mut genes = [0u64; 4];
        for gene in state.genomes.iter().flatten() {
            genes[gene.kind as usize] += 1;
        }
        let mut site = vec![0u64; state.k];
        for stocks in state.site_stock.chunks_exact(state.k) {
            for (total, &x) in site.iter_mut().zip(stocks) {
                *total = total.saturating_add(x);
            }
        }
        let contained = state.entities.iter().filter(|e| e.parent.is_some()).count() as u64;
        Row { tick: state.tick, entities: state.entities.len() as u64, contained, genes, site }
    }
}

#[derive(Default)]
pub struct History {
    pub rows: Vec<Row>,
    stride: u64,
}

impl History {
    fn add(&mut self, state: &State, last: bool) {
        self.stride = self.stride.max(1);
        if !(last || state.tick.is_multiple_of(self.stride)) || self.rows.last().is_some_and(|r| r.tick == state.tick) {
            return;
        }
        self.rows.push(Row::of(state));
        if self.rows.len() > HISTORY_POINTS && !last {
            self.stride *= 2;
            let stride = self.stride;
            self.rows.retain(|r| r.tick.is_multiple_of(stride));
        }
    }
}

#[derive(Default)]
pub struct Outcome {
    pub done: bool,
    pub final_hash: Option<String>,
    pub error: Option<String>,
}

pub struct Shared {
    pub snapshot: Mutex<Arc<State>>,
    pub history: Mutex<History>,
    pub outcome: Mutex<Outcome>,
    /// Labels of the metabolic types seen in this run; a type's code is its position plus one.
    pub types: Mutex<Vec<String>>,
    pub gate: Gate,
    pub resources: Vec<String>,
    pub width: u32,
    pub height: u32,
}

pub struct LiveRun {
    pub shared: Arc<Shared>,
    world: String,
    run: String,
    end_tick: u64,
    thread: Option<JoinHandle<()>>,
}

impl LiveRun {
    /// Ends the run (if still going) and waits until its files are written.
    fn finish(&mut self) {
        self.shared.gate.command("stop");
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Creates `results/studio/<world>-<n>` for the first unused n.
fn new_run_dir(results: &Path, world: &str) -> std::io::Result<(String, PathBuf)> {
    let root = results.join("studio");
    std::fs::create_dir_all(&root)?;
    for n in 1u32.. {
        let name = format!("{world}-{n}");
        let dir = root.join(&name);
        match std::fs::create_dir(&dir) {
            Ok(()) => return Ok((name, dir)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    unreachable!("fewer than 2^32 runs fit on a disk")
}

/// The world's text with the seed replaced, if one was asked for.
fn config_text(path: &Path, seed: &Value) -> Result<String, Reply> {
    let text = std::fs::read_to_string(path).map_err(|e| error(500, e))?;
    let Some(seed) = seed.as_str().map(str::trim).filter(|s| !s.is_empty()) else { return Ok(text) };
    let seed: i64 = seed.parse().ok().filter(|&s| s >= 0).ok_or_else(|| error(400, "seed must be a whole number below 2^63"))?;
    let mut table: toml::Table = text.parse().map_err(|e| error(400, e))?;
    table.insert("seed".into(), toml::Value::Integer(seed));
    toml::to_string(&table).map_err(|e| error(500, e))
}

pub fn start(studio: &mut Studio, body: &Value) -> Result<Reply, Reply> {
    let source = named_toml(&studio.folders.worlds, body["world"].as_str())?;
    let world = body["world"].as_str().unwrap_or_default().to_string();
    let whole = |key: &str| body[key].as_u64().filter(|&n| n > 0).ok_or_else(|| error(400, format!("{key} must be a whole number of at least 1")));
    let (ticks, threads) = (whole("ticks")?, whole("threads")?.min(256) as usize);
    if ticks > u64::from(u32::MAX) {
        return Err(error(400, "ticks must be below 2^32"));
    }
    let text = config_text(&source, &body["seed"])?;
    // Check the world before creating anything.
    let (cfg, _) = lab_core::config::parse(&text).map_err(|e| error(400, e))?;
    lab_rules::build(&cfg).map_err(|e| error(400, e))?;
    if let Some(mut old) = studio.live.take() {
        old.finish();
    }
    let (run, dir) = new_run_dir(&studio.folders.results, &world).map_err(|e| error(500, e))?;
    let config = dir.join("config.toml");
    std::fs::write(&config, text).map_err(|e| error(500, e))?;
    let mut session = Session::fresh(&config, threads).map_err(|e| error(400, e))?;
    let shared = Arc::new(Shared {
        snapshot: Mutex::new(Arc::new(session.state.clone())),
        history: Mutex::new(History::default()),
        outcome: Mutex::new(Outcome::default()),
        types: Mutex::new(Vec::new()),
        gate: Gate::new(body["paused"].as_bool().unwrap_or(false)),
        resources: session.cfg.world.resources.clone(),
        width: session.cfg.world.width,
        height: session.cfg.world.height,
    });
    let theirs = Arc::clone(&shared);
    let series_every = Some((ticks / 1000).max(1));
    let thread = std::thread::spawn(move || {
        let mut published = Instant::now();
        let publish = |state: &State| *theirs.snapshot.lock().expect("snapshot lock") = Arc::new(state.clone());
        let outputs = Outputs { checkpoint_every: None, series_every, observe: None };
        let result = execute(&mut session, ticks, &dir, outputs, |state| {
            theirs.history.lock().expect("history lock").add(state, false);
            if theirs.gate.paused() || published.elapsed() >= PUBLISH_EVERY {
                publish(state);
                published = Instant::now();
            }
            state.tick >= ticks || theirs.gate.wait()
        });
        publish(&session.state);
        theirs.history.lock().expect("history lock").add(&session.state, true);
        let mut outcome = theirs.outcome.lock().expect("outcome lock");
        match result {
            Ok(hash) => outcome.final_hash = Some(hex(&hash)),
            Err(e) => outcome.error = Some(e.to_string()),
        }
        outcome.done = true;
    });
    studio.live = Some(LiveRun { shared, world, run: run.clone(), end_tick: ticks, thread: Some(thread) });
    Ok(json_response(200, json!({ "run": run })))
}

pub fn control(live: Option<&LiveRun>, body: &Value) -> Result<Reply, Reply> {
    let live = live.ok_or_else(|| error(404, "no run has been started"))?;
    if let Some(speed) = body["speed"].as_u64() {
        live.shared.gate.set_speed(speed.min(1_000_000));
    } else if !body["command"].as_str().is_some_and(|c| live.shared.gate.command(c)) {
        return Err(error(400, "send {\"command\": \"pause\" | \"resume\" | \"step\" | \"stop\"} or {\"speed\": ticks per second}"));
    }
    Ok(json_response(200, json!({ "ok": true })))
}

pub fn status(live: Option<&LiveRun>) -> Value {
    let Some(live) = live else { return json!({ "active": false }) };
    let shared = &live.shared;
    // Read the outcome first: once it says done, the snapshot already holds the final state.
    let (done, final_hash, failure) = {
        let o = shared.outcome.lock().expect("outcome lock");
        let died = !o.done && live.thread.as_ref().is_some_and(JoinHandle::is_finished);
        (o.done || died, o.final_hash.clone(), if died { Some("the run stopped unexpectedly".to_string()) } else { o.error.clone() })
    };
    let state = Arc::clone(&shared.snapshot.lock().expect("snapshot lock"));
    json!({
        "active": true,
        "world": live.world,
        "run": live.run,
        "tick": state.tick,
        "ticks_total": live.end_tick,
        "paused": shared.gate.paused(),
        "speed": shared.gate.speed(),
        "done": done,
        "stopped": done && failure.is_none() && state.tick < live.end_tick,
        "final_hash": final_hash,
        "error": failure,
        "width": shared.width,
        "height": shared.height,
        "resources": shared.resources,
        "entities": state.entities.len(),
        "contained": state.entities.iter().filter(|e| e.parent.is_some()).count(),
    })
}
