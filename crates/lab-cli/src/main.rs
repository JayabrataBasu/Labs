//! `lab`: command-line front end of the evolutionary laboratory.
mod counterfactual;
mod edit;
mod observe;
mod run;
mod view;

use clap::{Parser, Subcommand};
use lab_core::LabError;
use run::{checkpoint_hash, dump_entities, execute, hex, Outputs, Session};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "lab", version, about = "Deterministic evolutionary laboratory")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run a config from tick 0.
    Run {
        config: PathBuf,
        #[arg(long)]
        ticks: u64,
        #[arg(long)]
        threads: usize,
        #[arg(long)]
        out: PathBuf,
        #[arg(long)]
        checkpoint_every: Option<u64>,
        /// Write population and resource totals to series.csv every this many ticks.
        #[arg(long)]
        series_every: Option<u64>,
        /// Record who took and released what over the last `observe-count` windows of this many ticks.
        #[arg(long, requires = "observe_count")]
        observe_window: Option<u64>,
        #[arg(long, requires = "observe_window")]
        observe_count: Option<u64>,
    },
    /// Continue a run from a checkpoint, with the config that produced it.
    Resume {
        checkpoint: PathBuf,
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        ticks: u64,
        #[arg(long)]
        threads: usize,
        #[arg(long)]
        out: PathBuf,
        #[arg(long)]
        checkpoint_every: Option<u64>,
        /// Write population and resource totals to series.csv every this many ticks.
        #[arg(long)]
        series_every: Option<u64>,
    },
    /// Fork a checkpoint keeping only chosen entities, and report what they achieve.
    Counterfactual {
        checkpoint: PathBuf,
        #[arg(long)]
        config: PathBuf,
        /// JSON: {"sets": [{"name": "...", "keep": [ids] or null, "report": [ids]}]}
        #[arg(long)]
        sets: PathBuf,
        #[arg(long)]
        ticks: u64,
        #[arg(long)]
        threads: usize,
        #[arg(long)]
        out: PathBuf,
    },
    /// Run a config from tick 0 while serving a read-only viewer on 127.0.0.1.
    View {
        config: PathBuf,
        #[arg(long)]
        ticks: u64,
        #[arg(long)]
        threads: usize,
        #[arg(long)]
        port: u16,
        #[arg(long)]
        out: PathBuf,
        /// Keep serving the final state for this long after the run ends.
        #[arg(long)]
        linger_seconds: u64,
        #[arg(long)]
        checkpoint_every: Option<u64>,
        /// Write population and resource totals to series.csv every this many ticks.
        #[arg(long)]
        series_every: Option<u64>,
        /// Hold at tick 0 until the page (or a POST to /control) resumes the run.
        #[arg(long)]
        start_paused: bool,
    },
    /// Open the visual world editor for a config file (127.0.0.1 only).
    Edit {
        config: PathBuf,
        #[arg(long)]
        port: u16,
    },
    /// Print the tick and state hash of a checkpoint.
    Hash { checkpoint: PathBuf },
    /// Write one CSV row per entity of a checkpoint (identity, lineage, stocks, genome).
    Dump {
        checkpoint: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
}

fn dispatch(cli: Cli) -> Result<(), LabError> {
    match cli.command {
        Command::Run { config, ticks, threads, out, checkpoint_every, series_every, observe_window, observe_count } => {
            let mut session = Session::fresh(&config, threads)?;
            let observe = observe_window.zip(observe_count).map(|(window, count)| observe::ObserveCfg { window, count });
            let hash = execute(&mut session, ticks, &out, Outputs { checkpoint_every, series_every, observe }, |_| {})?;
            println!("final tick={} hash={}", session.state.tick, hex(&hash));
        }
        Command::Resume { checkpoint, config, ticks, threads, out, checkpoint_every, series_every } => {
            let mut session = Session::resumed(&checkpoint, &config, threads)?;
            let hash = execute(&mut session, ticks, &out, Outputs { checkpoint_every, series_every, observe: None }, |_| {})?;
            println!("final tick={} hash={}", session.state.tick, hex(&hash));
        }
        Command::View { config, ticks, threads, port, out, linger_seconds, checkpoint_every, series_every, start_paused } => {
            let mut session = Session::fresh(&config, threads)?;
            let options = view::ViewOptions { ticks, out: &out, checkpoint_every, series_every, port, linger_seconds, start_paused };
            view::view(&mut session, &options)?;
        }
        Command::Edit { config, port } => edit::edit(&config, port)?,
        Command::Counterfactual { checkpoint, config, sets, ticks, threads, out } => {
            let forks = counterfactual::run(&checkpoint, &config, &sets, ticks, threads, &out)?;
            println!("wrote {forks} forks to {}", out.display());
        }
        Command::Dump { checkpoint, out } => {
            let rows = dump_entities(&checkpoint, &out)?;
            println!("wrote {rows} entities to {}", out.display());
        }
        Command::Hash { checkpoint } => {
            let (tick, hash) = checkpoint_hash(&checkpoint)?;
            println!("tick={tick} hash={}", hex(&hash));
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    match dispatch(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("lab: {error}");
            ExitCode::FAILURE
        }
    }
}
