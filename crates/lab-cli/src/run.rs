//! Running, checkpointing and resuming a simulation, and writing its manifest.
use lab_core::config::{self, Config};
use lab_core::{decode, encode, state_hash, Engine, LabError, State, FORMAT_VERSION};
use serde_json::json;
use std::path::Path;
use std::time::Instant;

pub struct Session {
    pub cfg: Config,
    pub config_text: String,
    pub config_hash: [u8; 32],
    pub engine: Engine,
    pub state: State,
    pub threads: usize,
}

impl Session {
    /// A new run starting at tick 0.
    pub fn fresh(config_path: &Path, threads: usize) -> Result<Self, LabError> {
        let (cfg, config_text, config_hash) = load_config(config_path)?;
        let state = config::initial_state(&cfg)?;
        Self::assemble(cfg, config_text, config_hash, state, threads)
    }

    /// Continues from a checkpoint. Refuses a config other than the one that produced it.
    pub fn resumed(checkpoint: &Path, config_path: &Path, threads: usize) -> Result<Self, LabError> {
        let (cfg, config_text, config_hash) = load_config(config_path)?;
        let (state, stored_hash) = decode(&read(checkpoint)?)?;
        if stored_hash != config_hash {
            return Err(LabError::Config(format!(
                "config hash mismatch: the checkpoint was produced with config {}, but {} has hash {}",
                hex(&stored_hash),
                config_path.display(),
                hex(&config_hash)
            )));
        }
        Self::assemble(cfg, config_text, config_hash, state, threads)
    }

    fn assemble(cfg: Config, config_text: String, config_hash: [u8; 32], state: State, threads: usize) -> Result<Self, LabError> {
        let engine = Engine::new(lab_rules::build(&cfg)?, cfg.seed, threads)?;
        Ok(Session { cfg, config_text, config_hash, engine, state, threads })
    }

    pub fn hash(&self) -> [u8; 32] {
        state_hash(&self.state, &self.config_hash)
    }
}

/// Advances `ticks` ticks, writing checkpoints and finally the manifest into `out`.
/// `after_tick` observes the state after each tick (and once before the first); it cannot change it.
pub fn execute(
    session: &mut Session,
    ticks: u64,
    out: &Path,
    checkpoint_every: Option<u64>,
    series_every: Option<u64>,
    mut after_tick: impl FnMut(&State),
) -> Result<[u8; 32], LabError> {
    let start_tick = session.state.tick;
    let end_tick = start_tick.checked_add(ticks).filter(|&t| t <= u64::from(u32::MAX)).ok_or_else(|| {
        LabError::Config(format!("tick limit exceeded: starting at tick {start_tick}, {ticks} more ticks would pass 2^32 - 1"))
    })?;
    if checkpoint_every == Some(0) {
        return Err(LabError::Config("--checkpoint-every must be at least 1".into()));
    }
    if series_every == Some(0) {
        return Err(LabError::Config("--series-every must be at least 1".into()));
    }
    std::fs::create_dir_all(out).map_err(|e| LabError::Io(format!("cannot create {}: {e}", out.display())))?;
    let mut series = series_every.map(|_| series_header(&session.cfg.world.resources));
    let record = |state: &State, series: &mut Option<String>| {
        if let (Some(every), Some(text)) = (series_every, series.as_mut()) {
            if state.tick == end_tick || state.tick.is_multiple_of(every) {
                series_row(text, state);
            }
        }
    };
    record(&session.state, &mut series);

    let mut checkpoints = Vec::new();
    let mut entity_ticks: u64 = 0;
    let started = Instant::now();
    after_tick(&session.state);
    while session.state.tick < end_tick {
        entity_ticks = entity_ticks.saturating_add(session.state.entities.len() as u64);
        session.engine.step(&mut session.state)?;
        after_tick(&session.state);
        record(&session.state, &mut series);
        let tick = session.state.tick;
        if tick == end_tick || checkpoint_every.is_some_and(|every| tick.is_multiple_of(every)) {
            let bytes = encode(&session.state, &session.config_hash);
            let path = out.join(format!("ckpt-{tick}.bin"));
            std::fs::write(&path, &bytes).map_err(|e| LabError::Io(format!("cannot write {}: {e}", path.display())))?;
            checkpoints.push(json!({ "tick": tick, "sha256": hex(&session.hash()) }));
        }
    }
    let elapsed_ms = started.elapsed().as_millis().max(1) as u64;
    let final_hash = session.hash();
    if ticks == 0 {
        let bytes = encode(&session.state, &session.config_hash);
        let path = out.join(format!("ckpt-{end_tick}.bin"));
        std::fs::write(&path, &bytes).map_err(|e| LabError::Io(format!("cannot write {}: {e}", path.display())))?;
        checkpoints.push(json!({ "tick": end_tick, "sha256": hex(&final_hash) }));
    }

    if let Some(text) = series {
        let path = out.join("series.csv");
        std::fs::write(&path, text).map_err(|e| LabError::Io(format!("cannot write {}: {e}", path.display())))?;
    }
    let rules: Vec<_> = session.engine.rules().iter().map(|r| json!({ "id": r.id(), "version": r.version(), "stream": r.stream() })).collect();
    let manifest = json!({
        "lab_version": env!("CARGO_PKG_VERSION"),
        "git_commit": env!("LAB_GIT_COMMIT"),
        "git_dirty": env!("LAB_GIT_DIRTY"),
        "rustc": env!("LAB_RUSTC"),
        "target": env!("LAB_TARGET"),
        "format_version": FORMAT_VERSION,
        "config_text": session.config_text,
        "config_sha256": hex(&session.config_hash),
        "seed": session.cfg.seed,
        "rng": "philox4x32-10; key=seed; ctr=(tick,subject,stream,index); stream 0 = initial placement",
        "rules": rules,
        "threads": session.threads,
        "start_tick": start_tick,
        "end_tick": end_tick,
        "checkpoints": checkpoints,
        "final_sha256": hex(&final_hash),
        "entity_ticks_per_second": entity_ticks.saturating_mul(1000) / elapsed_ms,
        "elapsed_ms": elapsed_ms,
    });
    let path = out.join("manifest.json");
    let text = serde_json::to_string_pretty(&manifest).map_err(|e| LabError::Io(e.to_string()))?;
    std::fs::write(&path, text + "\n").map_err(|e| LabError::Io(format!("cannot write {}: {e}", path.display())))?;
    Ok(final_hash)
}

/// Column names of `series.csv`. Everything is an exact integer; ratios are left to analysis.
fn series_header(resources: &[String]) -> String {
    let mut header = String::from("tick,entities,births,deaths,distinct_genomes,genes,uptake_genes,convert_genes,secrete_genes");
    for name in resources {
        header += &format!(",site_{name},entity_{name},influx_{name},dissipated_{name},converted_in_{name},converted_out_{name}");
    }
    header + "\n"
}

fn series_row(text: &mut String, state: &State) {
    let mut kinds = [0u64; 3];
    for gene in state.genomes.iter().flatten() {
        kinds[gene.kind as usize] += 1;
    }
    let mut genomes: Vec<&Vec<lab_core::Gene>> = state.genomes.iter().collect();
    genomes.sort_unstable_by_key(|g| g.iter().map(|x| (x.kind as u8, x.a, x.b, x.sense, x.level, x.threshold)).collect::<Vec<_>>());
    genomes.dedup();
    *text += &format!(
        "{},{},{},{},{},{},{},{},{}",
        state.tick,
        state.entities.len(),
        state.births,
        state.deaths,
        genomes.len(),
        kinds.iter().sum::<u64>(),
        kinds[0],
        kinds[1],
        kinds[2]
    );
    for r in 0..state.k {
        let on_sites: u128 = state.site_stock.iter().skip(r).step_by(state.k).map(|&x| u128::from(x)).sum();
        let in_entities: u128 = state.ent_stock.iter().skip(r).step_by(state.k).map(|&x| u128::from(x)).sum();
        let l = &state.ledger;
        *text += &format!(",{on_sites},{in_entities},{},{},{},{}", l.influx[r], l.dissipated[r], l.converted_in[r], l.converted_out[r]);
    }
    text.push('\n');
}

/// Writes one CSV row per entity of a checkpoint: identity, lineage, stocks and genome.
/// Genome syntax: genes separated by `;`, each `kind:a>b@sense>=threshold*level` with resource indices.
pub fn dump_entities(checkpoint: &Path, out: &Path) -> Result<usize, LabError> {
    let (state, _) = decode(&read(checkpoint)?)?;
    let mut text = String::from("id,site,x,y,mother,born,genes");
    for r in 0..state.k {
        text += &format!(",stock_{r}");
    }
    text += ",genome\n";
    for (i, e) in state.entities.iter().enumerate() {
        let mother = e.mother.map_or(String::new(), |m| m.to_string());
        text += &format!("{},{},{},{},{},{},{}", e.id, e.site, e.site % state.topo.width, e.site / state.topo.width, mother, e.born, state.genomes[i].len());
        for r in 0..state.k {
            text += &format!(",{}", state.ent_stock[i * state.k + r]);
        }
        let genome: Vec<String> = state.genomes[i]
            .iter()
            .map(|g| {
                let sense = if g.sense == lab_core::SENSE_ALWAYS { "always".to_string() } else { format!("{}>={}", g.sense, g.threshold) };
                format!("{}:{}>{}@{}*{}", g.kind.name(), g.a, g.b, sense, g.level)
            })
            .collect();
        text += &format!(",{}\n", genome.join(";"));
    }
    std::fs::write(out, text).map_err(|e| LabError::Io(format!("cannot write {}: {e}", out.display())))?;
    Ok(state.entities.len())
}

/// Decodes a checkpoint and returns its tick and state hash.
pub fn checkpoint_hash(checkpoint: &Path) -> Result<(u64, [u8; 32]), LabError> {
    let (state, config_hash) = decode(&read(checkpoint)?)?;
    Ok((state.tick, state_hash(&state, &config_hash)))
}

fn load_config(path: &Path) -> Result<(Config, String, [u8; 32]), LabError> {
    let text = std::fs::read_to_string(path).map_err(|e| LabError::Io(format!("cannot read {}: {e}", path.display())))?;
    let (cfg, hash) = config::parse(&text)?;
    Ok((cfg, text, hash))
}

fn read(path: &Path) -> Result<Vec<u8>, LabError> {
    std::fs::read(path).map_err(|e| LabError::Io(format!("cannot read {}: {e}", path.display())))
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
