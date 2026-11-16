//! `lab`: command-line front end of the evolutionary laboratory.
mod run;

use clap::{Parser, Subcommand};
use lab_core::LabError;
use run::{checkpoint_hash, execute, hex, Session};
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
    },
    /// Print the tick and state hash of a checkpoint.
    Hash { checkpoint: PathBuf },
}

fn dispatch(cli: Cli) -> Result<(), LabError> {
    match cli.command {
        Command::Run { config, ticks, threads, out, checkpoint_every } => {
            let mut session = Session::fresh(&config, threads)?;
            let hash = execute(&mut session, ticks, &out, checkpoint_every, |_| {})?;
            println!("final tick={} hash={}", session.state.tick, hex(&hash));
        }
        Command::Resume { checkpoint, config, ticks, threads, out, checkpoint_every } => {
            let mut session = Session::resumed(&checkpoint, &config, threads)?;
            let hash = execute(&mut session, ticks, &out, checkpoint_every, |_| {})?;
            println!("final tick={} hash={}", session.state.tick, hex(&hash));
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
