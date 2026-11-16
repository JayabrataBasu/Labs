# M2 + M3 report — evolution, null worlds, experiment layer, world editor

Date: 2026-11-16. Branch `m2-evolution` (not merged, not pushed). Built as **features for testing and refinement**:
the owner asked for the features to be created first and then tested and refined by a separate session.
Status tags: **[V]** verified by output in this session, **[U]** not verified.

## 1. What was built

| Area | Contents |
|---|---|
| Parallel apply (DR-014/015) | Deltas apply per site with defined semantics (`crates/lab-core/src/apply.rs` header), so results are independent of chunking and threads. Differentially tested against a plain sequential oracle |
| Entities | Genomes (variable-length typed genes: uptake, convert, secrete, with optional sensing gate), lineage (`mother`, `born`), births/deaths counters; checkpoint format v2 (DR-016) |
| Conservation | Now `total + dissipated + converted_out == initial + influx + converted_in`, exact, checked every tick |
| Rules (14 total) | M1 five plus `metabolism.genes`, `reproduction.division`, `reproduction.random`, `death.starvation`, `death.random`, `decay.fraction`, `mix.relocate`, `shock.scheduled`, `shock.cull` |
| Mutation (DR-006) | Four independent operators with their own rates: `point` (magnitude), `retarget` (identity), `duplicate`, `delete`, and a `max_genes` cap |
| Chemistry (DR-017) | The world lists allowed reactions; genes may mutate to any pair but only listed pairs do anything |
| Outputs | `series.csv` (`--series-every`), `lab dump` (one row per entity with lineage and genome), manifests as before |
| Null worlds (DR-007) | N1 no secretion, N2 well mixed, N3 neutral demography, N5 no mutation, as config transforms (`python/evolab/transforms.py`). N4 waits for the group detector (M4) |
| Python experiment layer | Stdlib only. `python -m evolab freeze/run/report/check` (see `experiments/rq1-baseline.toml`). Frozen specs (DR-008), full-factorial sweeps, parallel runs, summary CSV, distributions over seeds, construct check |
| World editor (M3) | `lab edit <config> --port N`: browser page with a map (drag to paint resource patches or shock rectangles), founder-genome editor, ordered rule list with a catalog, save. Validates with the same code a run uses; keeps `.bak`; localhost only, Host and Content-Type checks |
| Perturbation schedules | `shock.scheduled`, `shock.cull`, created from the editor's map or the rule list |

## 2. Verified [V]

- **Rust: 85 tests passed, 0 failed; clippy clean** (`cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`).
- **Python: 14 tests passed** (`PYTHONPATH=python python3 -m unittest discover -s python/tests`), including an end-to-end experiment: frozen spec, 4 runs, identical hashes whether run with 2 or 3 parallel jobs, all four runs distinct.
- **M1 CI passed on Linux, macOS and Windows** for the M1 commit (hash `8729cb30…329191` at 1 and 4 threads on all three). This is the cross-platform evidence for the kernel as of M1. **The M2 kernel has not yet run on CI**, and the reference hash was deliberately re-pinned (`b31c045e…ecc9f`, checkpoint format v2), so the CI replay for M2 is untested until pushed.
- Reference replay on this machine after M2: threads 1 and 4 both give `b31c045e…ecc9f`.
- Editor: opened in headless Chromium, screenshot `docs/img/m3-editor.png` (world fields, stocks, genome editor, map, rule list).
- Property tests (random worlds including all lifecycle rules, 64 cases each): thread invariance, exact conservation (new identity), stream isolation, resume equivalence, entity bookkeeping (`entities == founders + births − deaths`).
- **Speed:** parallel apply gave 7.8×10⁶ entity-ticks/s at 12 threads on the M1 benchmark world (was 6.4×10⁶): only about 20% better. A phase timing showed index building was not the cost; propose ≈ 40 ms and apply ≈ 55 ms of ≈ 105 ms per tick across 5 rules, dominated by the entity-heavy rules (uptake, maintenance, movement). **DR-014's acceptance criterion "measured speed-up" is met only weakly.** Further gains need per-rule work (e.g. avoiding the entity-stock gather/scatter, cheaper per-site allocation), not more threads. The M0 spike's 4.5×10⁷ is still about 6× away.

## 3. First experiment and what it shows [V, exploratory]

`experiments/rq1-baseline.toml`, run with `--exploratory` (not registered): 4 arms × 5 seeds, 2,000 ticks, `configs/crossfeed.toml` (48×48, 600 founders, not tuned).

```
== entities (final population)
N1_no_interaction   n=5  mean 4493.6  sd 17.3  [4476, 4522]
N2_well_mixed       n=5  mean 4502.2  sd 21.8  [4481, 4537]
N5_no_mutation      n=5  mean 3167.0  sd  8.7  [3156, 3177]
intact              n=5  mean 4502.2  sd 18.7  [4477, 4522]
== secrete_genes (final)
N1: 0   N2: 0.0   N5: 3167 (every entity keeps the founder gene)   intact: 0.4 (0 to 1 gene in 4,500 entities)
construct check (entities): all four checks PASS
```

Reading it, and what it does **not** say:
- In this world the secretion gene is **selected out even in the intact arm**: leaking intermediate costs the leaker and mostly benefits others (free-rider outcome). Population size in the intact world is statistically indistinguishable from N1, where secretion is silent. **No interaction-dependent organization appeared** at these settings. That is a result about *this parameterisation*, not about RQ1; the world was never tuned for cooperation (RQ1 needs conditions where it can pay, e.g. stronger spatial structure or kin clustering).
- N5 (no mutation) is smaller because founders cannot shed the costly gene.
- **N1 is not a complete null.** A dying entity returns its stocks to its site, where neighbours can take them up. That recycling channel stays on in N1. In the small end-to-end test B appeared on sites in N1 only by this route. Whether N1 should also disable corpse recycling is a design question for the owner (proposed below).
- Five seeds per cell is the floor, not enough for any inferential claim.

## 4. Not verified / known gaps [U]

- **CI for the M2 kernel**; Windows and macOS behaviour of the new code, including the Python tests (which run on Linux only in CI).
- **N3 (neutral)** is implemented but its rates must be matched by hand to the intact arm's demography; no experiment uses it yet. N2 only scatters *entities*; resources still diffuse, so it is a proxy for "well mixed".
- **Editor in a real browser by a person**; Firefox; touch. It was exercised in headless Chromium by screenshot only, plus server tests. Resource renaming does not update references (the page warns).
- **No group detector, synergy or Price metrics** (M4). The Python `check` only tests that an outcome varies, that an intact arm and null arms exist, and seeds per cell.
- **Performance:** see §2; large worlds with many rules are still slow.
- `lab view` (live viewer) does not show genomes or lineage; it shows one resource and entity counts.
- Sweeps may only change settings that already exist in the base config.
- Editor limits: integers above 2⁵³ are refused; the seed above 2⁶³−1 is refused (TOML); rule parameters are edited as JSON text.
- DR-015 changed the meaning of "ordered second pass" from DR-014. It was implemented under DR-014's approval and needs the owner's confirmation.

## 5. Decisions

DR-015 to DR-019 are in `docs/DECISIONS.md`, decided by Claude on the owner's delegation ("make the decisions for me"). Reasoning and evidence in §7. The owner can supersede any of them.

## 6. Handoff for the tester

Start here: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo build --release`, `PYTHONPATH=python python3 -m unittest discover -s python/tests`.
Try it: `./target/release/lab edit configs/crossfeed.toml --port 8788`, then `lab run … --series-every 100`, then `lab view …`.
Worth attacking: the apply semantics (`crates/lab-core/src/apply.rs`) for ordering corner cases; checkpoint decode with hostile bytes; the editor's save path; whether property-test worlds really cover births and deaths (add counters to the generator); `retarget` mutation producing degenerate genomes; sweep paths; the N-arm transforms against rules they do not expect.

## 7. Decisions made on the owner's behalf, with the evidence

Sources were found by web search; abstract-level reading only unless noted, and no claim below depends on details I did not read.

**DR-018 (recycle flag; N1 = N1a + N1b).** The first baseline showed an inter-entity channel the design had not named: dead entities return their stocks to the site. A test in the Python layer confirmed it (full N1 now keeps resource B off every site; the intact arm does not). Literature on cross-feeding stresses that dependencies run through several routes and that spatial structure changes the predictions (McNally & Borenstein 2018; Stump et al. 2018 on neighbour uncertainty; segregation experiments, PMC5029186). My search found nothing on necromass specifically, so I make no claim about how large this channel is; DR-018 only makes it switchable and separable. *Alternative rejected:* silently turning recycling off, because dead biomass genuinely returns nutrients in many real systems and the lab should let that be a studied variable, not a hidden default.

**DR-019 (two-phase protocol, seed firewall, ADEMP fields, ODD at M4).**
- Siepe et al. (2026 preprint) argue simulation studies have many researcher degrees of freedom, that preregistration mainly clarifies which conditions were considered, and that it matters more for late-phase studies than for early-phase work on new methods. The Center for Open Science published a simulation-study preregistration template built on ADEMP. Conclusion: this lab is early-phase, so full preregistration of everything would be ritual; but the confirmatory step must be protected. Hence exploratory runs (seeds < 1000) may tune freely, and a registered run must be frozen with fresh seeds >= 1000. The earlier `~/firma` project used a seed firewall too (reserved seeds 1-200) and its review credited it with keeping exploration separable from the registered run.
- ODD (Grimm et al. 2020, JASSS 23(2):7) exists so a model can be re-implemented from text without its code; TRACE (Grimm et al. 2014) documents testing and evaluation. I adopt both, but write the ODD at M4: the model still changes every milestone, and a description of a moving target would be wrong within a week.
- Reproducibility practice in agent-based modelling (GAMA, LASER documentation; a 2026 arXiv preprint on paired counterfactual simulation) recommends separate random streams per process, allocation by agent id, and counter-based generators keyed to event identifiers. This lab already does exactly that (DR-003); no change needed. I did not find a source that contradicts it.
- *Alternatives rejected:* (a) requiring ODD now; (b) registering every experiment, which would make exploration impossible; (c) no firewall, which is the garden-of-forking-paths risk the lab's rules name.

**First exploratory sweep (DR-019 phase 1), 3 seeds per cell, 2,500 ticks, intact world only [V]:**
```
walk probability (ppm)   secrete genes at end (mean)     entities (mean)
0 (sessile)              ~1980 of ~2020 entities carry it    ~2020
20 000                   0.3 to 3                            ~4500
100 000                  0                                   ~4510
400 000                  0 to 0.7                            ~4500
```
(Two B-diffusion settings, divisor 8 and 60, gave the same picture.) **Reading:** in this world secretion survives only when entities do not move at all. This agrees in direction with the published finding that spatial segregation stabilises cross-feeding, but it is one world, three seeds, one dimension of mixing, and a sessile world also changes who is each entity's neighbour and how fast the population fills space. It tells me where to look (low motility, local reproduction), not why. **Not a result for RQ1.** The next exploratory steps are to sweep intermediate walk rates (1 000 to 20 000 ppm), secretion level and cost, and to add the N1a/N1b arms, before any registered design.

Consequences already in the code: `freeze` refuses seeds below 1000; specs must state `aim` and `estimand`; `experiments/rq1-baseline.toml` and `rq1-tuning-explore.toml` are exploratory (seeds 1 to 5, not freezable).

Sources: ODD 2020 (https://www.jasss.org/23/2/7.html); TRACE (https://ideas.repec.org/a/eee/ecomod/v280y2014icp129-139.html); simulation-study preregistration (https://www.cos.io/blog/introducing-the-simulation-studies-preregistration-template, and the Siepe et al. 2026 preprint at https://arxiv.org/pdf/2606.11217); spatial cross-feeding (https://pmc.ncbi.nlm.nih.gov/articles/PMC5029186, https://ui.adsabs.harvard.edu/abs/2018JThBi.446...33S, https://pmc.ncbi.nlm.nih.gov/articles/PMC6003207); ABM reproducibility (https://gama-platform.org/wiki/Ensure-model-reproducibility, https://arxiv.org/html/2603.11084v1).
