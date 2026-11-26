# Programmable Artificial Evolutionary Laboratory

A laboratory in which you set up an artificial world (resources, chemistry, movement, reproduction,
containment), let genomes evolve in it, and test with controls and null worlds what organization
arises and why. The same configuration and seed give the same result, bit for bit, on Linux, macOS
and Windows at any number of threads.

A result here means that the stated rules and random process produced it. It is not a claim about
real organisms or organizations.

## Install

You need Rust (the pinned version installs itself through `rustup`) and Python 3.11 or newer.
No Python packages are needed.

```
cargo build --release
```

Ready-built packages for Linux, macOS and Windows are attached to each run of the `replay` workflow
on GitHub (Actions → a run → Artifacts). Unpack one, and run `target/release/lab` from its top folder.
On Linux and macOS you may first need `chmod +x target/release/lab`.

## Quickstart: the studio

```
./target/release/lab studio --port 8787
```

Open <http://127.0.0.1:8787/>. The server answers only on your own machine.

| Tab | What you can do |
|---|---|
| **Worlds** | Edit a world (size, resources, starting patches, founders and their genes, rules) and save it. "Copy" makes your own world from an example. |
| **Run** | Run a world and watch it: map by resource, crowding, guests or metabolic type; click a site to read its entities' genomes, stocks and family; population, gene and resource charts; pause, step, slow down, stop. |
| **Experiments** | Run an experiment specification from `experiments/`, follow its progress, and open its report. |

Watching does not change a run: a world run in the studio ends in the same state hash as on the
command line. Runs started here are saved in `results/studio/<world>-<n>/`. Experiments started here
are marked *exploratory*.

Other options: `--worlds`, `--results`, `--experiments` (folders), `--python` and `--python-bin`
(where the Python layer and interpreter are). On Windows the interpreter default is `python`.

## Command line

```
./target/release/lab run configs/blackqueen.toml --ticks 2000 --threads 1 --series-every 100 --out runs/bq
./target/release/lab resume runs/bq/ckpt-2000.bin --config configs/blackqueen.toml --ticks 500 --threads 1 --out runs/bq2
./target/release/lab hash runs/bq/ckpt-2000.bin
./target/release/lab dump runs/bq/ckpt-2000.bin --out runs/bq/entities.csv
./target/release/lab edit configs/blackqueen.toml --port 8788        # the editor on its own
./target/release/lab view configs/reference.toml --ticks 5000 --threads 4 --port 8789 --out runs/view --linger-seconds 600
```

Every run writes `manifest.json` (code version, full configuration, seed, rule versions, state
hashes) and checkpoints. `--threads 1` is the fastest choice for small worlds; run several
processes in parallel instead (the experiment runner does this).

## Experiments

An experiment is a TOML file naming a base world, arms (the intact world and its null worlds),
parameter sweeps, seeds, outcomes and an analysis plan. `python/evolab/spec.py` documents every field.

```
export PYTHONPATH=python
python3 -m evolab run experiments/demo.toml --out results --jobs 4 --exploratory   # about a minute
python3 -m evolab html results/demo                     # self-contained report.html with charts
python3 -m evolab report results/demo --metric dependent_fraction_leaked
python3 -m evolab check  results/demo --metric dependent_fraction_leaked
```

Exploratory runs use seeds below 1000 and may be used to adjust a world. A registered experiment is
first frozen (`python3 -m evolab freeze spec.toml`), uses seeds from 1000 up, and refuses to run if
the specification, base world or any expanded run config changed afterwards.

`LAB_BIN=/path/to/lab` selects the binary if it is not at `target/release/lab`.

## Example worlds (`configs/`)

| World | What it is for |
|---|---|
| `reference.toml` | Smallest world; the cross-platform replay anchor |
| `crossfeed.toml` | One resource chain with voluntary secretion |
| `blackqueen.toml` | Leaky production of two needed products; lineages may drop a step and depend on neighbours |
| `blackqueen-symmetric.toml` | The same with two equivalent leaky pathways |
| `containment.toml` | Entities may live inside one another (one level) |
| `positive-control.toml` | Two specialists built to depend on each other (instrument check) |
| `negative-control.toml` | Self-sufficient entities with no leaks (instrument check) |

## Adding a mechanism

Each rule is one file in `crates/lab-rules/src/`. Add the file, give it an id and a version, register
it in `lib.rs` (`RULE_IDS`, `CATALOG`, `build_one`). The editor and studio pick it up from the
catalog. A rule only *proposes* changes; `lab-core` applies them and checks conservation every tick.

## Where things are

| | |
|---|---|
| Mission and working rules | `CLAUDE.md` |
| Model description (ODD) | `docs/ODD.md` |
| How the model and instruments were tested | `docs/TESTING.md` |
| Decisions | `docs/DECISIONS.md` |
| Handoff for code review | `docs/BUG-SWEEP.md` |
| Scope and glossary | `docs/PROPOSAL.md` |
| Experiments and findings | `docs/experiments/` |
| Progress and rulings | `docs/plans/` |

Developing: `cargo test --workspace --release`, `cargo clippy --workspace --all-targets -- -D warnings`,
`PYTHONPATH=python python3 -m unittest discover -s python/tests`.
