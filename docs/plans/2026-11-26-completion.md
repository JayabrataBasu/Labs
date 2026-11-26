# Plan: from M3 to the finished laboratory

## Context
M0 to M3 are on `master`: a deterministic Rust kernel with evolving genomes, 14 rules, a Python experiment layer, a world editor and a live viewer, with cross-platform replay passing in CI. The owner wants the **working product**, built through to the end, with experiments and sweeps run along the way, and decisions made by me. An Opus bug sweep follows later, so this plan ends with a handoff for it.

**Missing today:** group detection and synergy/Price metrics (M4), a registered RQ1 result, containment (M5), one unified app, and user documentation.

**Scientific gap found while planning:** in the current world secretion is a purely costly voluntary act, so it is selected out unless entities never move. The Black Queen mechanism the proposal names rests on *unavoidably leaky* functions. The world also has only one essential product, so complementary dependence cannot arise. Phase A fixes both.

## Working rules
- Test first for every task; `cargo test`, clippy and the Python tests stay green.
- One branch per phase. Commit under the owner's name with **no trailers**. **Approving this plan authorizes me to push each branch and merge it to `master` once CI is green on all three platforms.**
- **Approving this plan also approves decisions DR-020 to DR-023 below.** They go in `docs/DECISIONS.md` marked "decided on delegation".
- No subagents. Progress is kept in `docs/plans/2026-11-26-completion.md` (a copy of this plan with checkboxes) so any later session can resume.
- I stop only for destructive actions, or if a result shows that a planned measure is invalid and no sound replacement exists.
- A negative or null scientific result is an acceptable outcome. I will not tune until a hypothesis passes.

## Phase A: world mechanisms for RQ1 (branch `m4a-world`, DR-020)
1. **Leaky reactions.** `metabolism.genes.reactions` becomes `[{from, to, leak_ppm}]`. `Delta::Convert` gains the leak (rounded by a keyed draw), applied in `crates/lab-core/src/apply.rs` from the entity to its site. Tests: exact amounts, conservation, thread invariance.
2. **Better no-transfer null.** `leak_to` / secretion target = `"site" | "sink"`. N1a sends leaked and secreted matter to the sink, so the producer's budget is unchanged and only transfer is removed. This replaces the current `secretion = false`.
3. **Several essential resources.** `reproduction.*division` takes `requires = [{resource, threshold}]`.
4. **Several founder types.** Config schema 2: `init.founders = [{count, stock, genome}]` replaces `entities`, `entity_stock` and `genome`. Migrate configs, editor, transforms, tests.
5. **World `configs/blackqueen.toml`:** A flows in; A→B, A→C, B→E, C→F; starvation on E; division needs E and F. Also `positive-control.toml` (two imposed complementary specialists) and `negative-control.toml` (generalists, no leak).
6. Re-pin replay hashes; CI replays all pinned worlds; remove the temporary branch trigger and run Python tests on all three platforms.

## Phase B: observation without interference (branch `m4b-observe`, DR-021)
- An optional observer in `apply.rs` accumulates, per window and per (entity, site, resource): amount taken, released (secretion, leak, death) and produced; plus birth and death events. `lab run --observe-every N` writes them under `out/obs/`.
- **Required property:** observer on or off gives identical state hashes; observed totals reconcile exactly with the ledgers.
- `lab counterfactual <checkpoint> --config … --sets sets.json --ticks H`: reruns forks keeping only listed entities, with the same random keys (reuses `Session::resumed` in `crates/lab-cli/src/run.rs`).

## Phase C: analysis (branch `m4c-analysis`, Python standard library only, DR-023)
New modules in `python/evolab/`:
| Module | Measures |
|---|---|
| `capability.py` | Functional type of each genome; auxotroph frequencies; complementary coexistence (exact, no detector needed) |
| `flux.py` | Who-fed-whom graph by local-pool attribution: uptake at a site split among releasers nearby in proportion to release. An **estimator**, stated as such |
| `groups.py` | Two independent detectors (reciprocal-edge components; weighted label propagation), persistence by member overlap across windows, N4 spatial shuffle |
| `metrics.py` | Specialization, encapsulation, persistence, agreement between detectors |
| `price.py` | Price decomposition, within vs between groups |
| `counterfactual.py` | Dependence (partner removal) and synergy (group vs solo forks, DR-009), with the resource-competition confound reported alongside |

**Construct-check gate:** both detectors and every primary measure must be high in the positive control, near zero in the negative control and under N4, and not fixed by the rules. If a measure fails, I repair or replace it before any experiment. Hand-computed unit tests cover each formula.

## Phase D: experiments (run in the background while building E and F)
1. **Benchmark** the RQ1 world. If the registered design would exceed 8 hours on this laptop, first optimize (store entities in site order; the profiled cost is random memory access), with hashes unchanged.
2. **Exploration** (seeds below 1000): sweeps over leak, gene cost, motility, diffusion, yield and inflow. At most three rounds, each logged in `docs/experiments/EXPLORATION.md` with what was changed and why.
3. **Calibrate N3** (neutral birth and death rates matched to the intact arm's measured demography) with a small tool.
4. **Literature check** on leakiness, spatial structure and detection by flux, before wording any claim.
5. **Register RQ1:** freeze the spec (hypotheses, primary outcomes, falsification thresholds, number of seeds chosen from exploratory variance), commit it, then run with seeds from 1000 up. Candidate hypotheses:
   - H1: locality changes the frequency of complementary auxotroph coexistence (intact vs N2).
   - H2: detected reciprocal coalitions persist longer than under the N4 shuffle.
   - H3: gene loss exceeds what neutral demography gives (intact vs N3).
   - N1 is reported only as a sanity control: dependants cannot live without transfer **by construction**, so it is not evidence.
6. **Report** `docs/experiments/RQ1-REPORT.md`: distributions over seeds, bootstrap intervals and permutation tests with fixed seeds, sensitivity sweep, what the model cannot establish.

## Phase E: containment, M5 (branch `m5-containment`, DR-022)
- Uses the reserved `parent` slot. A new `bind` gene lets co-located compatible entities form host and guest (one level deep). Guests move with the host and trade only with it; at host division each guest goes to the offspring with a set probability; host death releases guests.
- Rules: `containment.bind`, `containment.share`, `containment.release`. Costs apply; no benefit is built in.
- Nulls: containment without sharing; random re-assignment of guests at division (breaks inheritance).
- Property tests: conservation across binding, release and division; no cycles or orphaned guests; thread invariance; resume equivalence.
- **RQ1b, exploratory only:** does fitness variance move from within hosts to between hosts (Price)? Registering it is left for the owner's decision.

## Phase F: the product (branch `studio`)
- **`lab studio`**, one local web app joining `view.rs` and `edit.rs`:
  - **Worlds:** the editor, extended for founders, leaks, requirements and containment.
  - **Run:** live map coloured by resource, functional type, host or detected group; click an entity to see genome, stocks and lineage; population and gene charts; pause, step, speed.
  - **Experiments:** list results, open reports, start exploratory sweeps.
  - Local only, with the existing Host and Content-Type checks. Live viewing must leave hashes unchanged.
- `python -m evolab html <results>`: a self-contained report with charts.
- `README.md` quickstart (build, edit, run, sweep, report); example worlds; CI uploads ready-built binaries for Linux, macOS and Windows.
- `docs/ODD.md` (model description, DR-019) and a TRACE-style testing log.
- Browser check by screenshots in Chromium, and Firefox if available.

## Phase G: handoff for the Opus bug sweep
`docs/BUG-SWEEP.md`: how to run everything, invariants, riskiest code (apply semantics, observer, containment, codec on hostile input, studio server), every ruling I made, known gaps and unverified claims.

## Decisions approved with this plan
| # | Decision | Cost if wrong |
|---|---|---|
| DR-020 | Leaky reactions, sink-target null, multi-resource division, multiple founders (config schema 2) | Config migration; hashes re-pinned |
| DR-021 | Observation is read-only and outside the state hash; flux between entities is estimated by local-pool attribution and validated on controls | Detector may mislead; the construct gate limits this |
| DR-022 | One-level containment through a costly gene, with the two nulls | Rework of M5 only |
| DR-023 | Python drives the `lab` binary using the standard library only (supersedes the PyO3/NumPy part of DR-001) | Slower analysis of large runs |

## Order and checkpoints
A → B → C → D (starts) → F (studio core) → E → F (finish) → D report → G. After each phase I merge and report briefly with real output. This is a multi-session job; if a usage limit stops it, the progress file says where to resume.

## Verification
- Per phase: `cargo test --workspace --release`, `cargo clippy --workspace --all-targets -- -D warnings`, `PYTHONPATH=python python3 -m unittest discover -s python/tests`, CI green on Linux, macOS and Windows with every pinned hash.
- Science: construct-check output on positive and negative controls; frozen spec committed before registered runs; a fresh clone replays one registered run to the same hash.
- Product: from a clean clone, follow the README to author a world in the studio, run it, watch it, sweep it and open the report.

## Progress (update as work lands)
- [x] Phase A: world mechanisms (leak, release_to, requirements, founders, three new worlds)
- [x] Phase B: observer, counterfactual
- [ ] Phase C: analysis and construct-check gate
- [ ] Phase D: benchmark, exploration, N3 calibration, literature, registered RQ1, report
- [ ] Phase E: containment
- [ ] Phase F: studio, HTML report, README, ODD, binaries
- [ ] Phase G: bug-sweep handoff

### Rulings and findings
- A: first Black Queen parameters starved every population (yield 1/2); yield is now 3/4. Untuned.
- A: the first positive control collapsed because specialists also took their own product back; each now lacks uptake of what it makes.
- A (exploratory observation, seed 1, one run): in blackqueen.toml lineages that dropped a leaky step are common by tick 6000. Not evidence until compared with nulls over seeds.
