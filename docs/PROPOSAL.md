# Programmable Artificial Evolutionary Laboratory — Scope, Glossary, Technology Proposal

Status: **DRAFT for owner approval.** No engine code exists. Nothing here counts until approved.
Date: 2026-11-16. Stage: 0 (proposal). Decision records: none yet (proposed ones are in §8).

Labels used below: **[E]** established in the literature, **[S]** speculative/contested, **[M]** my design choice,
**[U]** unverified recall (a literature pass is scheduled before any of it is relied on; see §9).

---

## 1. What the owner decided (answers applied)

| Q | Resolution | Source |
|---|---|---|
| 1 | Research question (a): obligate interdependence → persistent specialized coalitions vs free-rider collapse. (b) follows later. | default |
| 2 | Agents in a conserved resource world | default |
| 3 | Variable-length genome, separately controlled mutation operators | default |
| 4 | Lattice/graph, synchronous discrete steps; well-mixed kept as a control | default |
| 5 | Strict conservation, tested as an invariant | default |
| 6 | No group primitives in the core; groups detected by analysis; bonding/membrane modules optional | default |
| 7 | Ablation + fitness decomposition first; information-theoretic as cross-check | default |
| 8 | Hardware measured: i5-1335U (10 cores/12 threads, throttles), 15 GiB RAM, no GPU, 37 GB free disk | measured |
| 9 | **Cross-machine reproducibility**: the same configuration must give the same result on someone else's system | owner, overrides my default |
| 10 | Rules are written by AI on the owner's request; owner edits configs and visuals, not code | owner |
| 12 | Prior attempt (`~/firma`) had too many ADRs; lessons in §2 | owner |
| 13 | No preferred hypothesis stated; all outcomes treated as acceptable | default |
| 14 | Resource exchange + one costly signalling channel | default |
| 15 | **Entities may fuse or contain each other** (owner: how firms and some organisms work). Included in the roadmap as M5, with the entity model designed for it from day one. | owner, overrides my default |
| 16 | Evolutionary adaptation only in v1; plasticity later as an ablatable module. I chose this because "most scientific" means keeping selection and learning separable so a factorial with/without design is possible [M]. | owner delegated |
| 17 | Static baseline; fluctuation/shocks as swappable rules; **world authored in a visual editor** that writes the declarative config | owner |
| 18 | Finite runs with checkpoint/resume | default |
| 19 | **Live visuals** (browser viewer) | owner |
| 11, 20 | Not answered; defaults: theory-building with publication-grade rigor available; `git init` locally at M1, private | default |

## 2. Lessons from the prior attempt (`~/firma`, read-only inspection)

I read `docs/review/REVIEW-2026-11-05.md`, `PROGRESS.md`, `docs/review/STATE.md` and the ADR index. I did not read the code. The lessons are stated as the prior project's own findings.

1. **Hypotheses true by construction.** The review found ΔH_rep ≡ 0 by construction in Arm A (625/625 identical action sequences), `search_width` independent of one factor by construction, and a hard-switch rule used to test "switching vs simultaneity". *Consequence here:* every experiment needs a **construct check** before any registered run. It must show that the outcome measure can come out *false* (negative control) and *true* (positive control), and that it is not fixed by the rules.
2. **Specification outran evidence.** A 2.7k-line manual and 66 ADRs existed before any first result; the project's own state file lists dozens of open owner decisions. *Consequence:* no manual until results exist; the spec stays at proposal size and grows only when a run needs it.
3. **ADR inflation.** Much of the decision volume was fixing interactions between earlier decisions. *Consequence:* §8 — one append-only file, a decision only when it changes a scientific interface or a hard-to-reverse architecture choice, and a soft cap (target ≤ 15 through M2).
4. **Reuse the engineering lessons, not the model.** Deterministic NDJSON logs, a keyed RNG crate, Rust kernel + Python (PyO3) lab, and a seed firewall all worked per the review ("the engineering is sound"). This lab is a new project and copies none of the code.

## 3. Scope

### 3.1 Mission in one sentence
A deterministic, configurable world in which resource-constrained entities evolve, with analysis tools that detect and measure higher-order organization *after the fact*, so that mechanisms (interdependence, specialization, information limits, encapsulation) can be manipulated and their effects compared with null models.

### 3.2 First research question (RQ1)
> Under what conditions does obligate interdependence between entities with complementary resource needs produce **persistent, specialized coalitions**, as opposed to free-rider collapse or no stable structure?

Two sub-questions, kept separate because they are different mechanisms:
- **RQ1-imposed:** dependence is built into the world (resource chain where no single genome can complete the cycle efficiently).
- **RQ1-evolved:** dependence arises through *gene loss* when a neighbour supplies the product (the Black Queen hypothesis [U]).
The contrast is the scientific content: imposed dependence is nearly trivial; evolved dependence is a real outcome.

### 3.3 In scope for the first build (M1–M4)
Conserved integer resource world; entities with variable-length genomes; reproduction, death, mutation; resource exchange and one costly signal; synchronous lattice/graph; null-model worlds; live viewer; visual world editor; experiment runner with provenance; group detection and first metrics.

### 3.4 Explicitly out of scope until a named question needs it
Plasticity/learning; predation/parasitism; horizontal gene transfer; continuous space; GPU; WASM plugin host; real-world calibration. (Fusion/containment is deferred to M5, not dropped.)

### 3.5 What the lab cannot establish (stated now, restated with every result)
- It shows that a *specified mechanism and stochastic process generated an outcome*, nothing about real firms or organisms.
- The biological → abstract → organizational translation chain (§3.6) is an analogy at each arrow, not a proof.
- A simulated "coalition" is whatever my detector says it is; detector choice is a confound and gets robustness checks.

### 3.6 Translation chain for RQ1 (rule 3)
| Biological mechanism | Abstract mechanism | Organizational construct | Empirical prediction (falsifiable) |
|---|---|---|---|
| Cross-feeding between microbes | Entity A exports product X at cost; entity B needs X | Supplier–customer dependence | Evolved dependence appears only above a supply-reliability threshold |
| Gene loss when product is public (Black Queen [U]) | Gene deletion with maintenance-cost saving | Outsourcing a capability | Loss rate ↑ with neighbour supply; collapse if suppliers go |
| Spatial structure limits free-riding [E: standard result in cooperation theory] | Local interaction/reproduction | Locality of relationships | Coalition persistence ↓ in well-mixed control |

## 4. Architecture

### 4.1 Layers (separable; each has a test interface)
1. **Primitives:** integer arithmetic, RNG, IDs, deterministic containers.
2. **World:** topology (lattice/graph), resource stocks per site, flux in, dissipation out.
3. **Entities:** body (internal stocks), genome, location, lineage id, `parent` slot (reserved for containment, M5).
4. **Rules (plugins):** uptake, conversion, secretion/exchange, signalling, maintenance cost, reproduction, death, mutation operators, perturbations. Each rule is versioned, declares what state it reads/writes, and is replaceable by config.
5. **Experiment layer:** config, sweeps, seeds, interventions, checkpoints, provenance.
6. **Analysis layer:** group detection, metrics, null-model comparison. Never feeds back into the kernel.
7. **Interface layer:** browser viewer and world editor. Contains no science; reads snapshots, writes configs.

### 4.2 Genome model [M]
A genome is a variable-length list of typed **genes**: uptake (resource type, capacity), conversion (A→B with yield < 1, the remainder dissipated), secretion (release a product to the site, at a cost), signal (emit/respond), regulatory threshold. Each gene has a maintenance cost per tick, so extra genes are never free. Mutation operators are independent and separately configured: point change (rate, magnitude distribution), gene deletion, duplication, insertion. This makes rule 6's distinctions measurable: mutation frequency, magnitude, structural change, genotype diversity (a descriptive Shannon measure), and phenotypic novelty are different quantities, and none is called "entropy budget" or thermodynamic entropy.

### 4.3 Conservation [M, enforces owner Q5]
Each resource type is an integer quantity. Per tick: `Σ stocks(t+1) = Σ stocks(t) + influx − dissipation`, checked exactly (no tolerance). Dissipation is accounted as a heat-like sink, which is a defensible flux quantity. No thermodynamic-entropy claims are made.

### 4.4 Reproducibility architecture (owner's Q9: cross-machine) [M]
Float arithmetic can differ across CPUs, compilers, libm versions and thread counts, so the kernel does not use it for state:
- **All simulation state is integer or fixed-point.** Probabilities are compared as integer thresholds against integer random draws. Floats appear only in analysis.
- **Counter-based RNG** (Philox-type [U, to be verified against published test vectors]), keyed by `(seed, stream, tick, entity id, purpose)`. No shared sequential generator, so results do not depend on thread count or scheduling.
- **Synchronous double-buffered updates:** entities propose deltas; deltas are applied in canonical order (sorted by id); conflicts resolve by keyed RNG, not by arrival order.
- No hash-order iteration; explicit little-endian serialization; pinned toolchain and lockfile; no fast-math.
- **State hash** (cryptographic) per checkpoint, stored in the run manifest. A replay on another machine is verified by hash equality.
- *Property tests:* same seed + different thread counts → identical hash; conservation holds after every tick; stream isolation (adding a rule that draws randomness does not change other rules' draws); viewer attached vs detached → identical hash.
- *Known limit:* I can verify on this machine and (via CI or a second machine, if available) on one other; "any system" is a claim I can only support by test on the platforms actually tried.

### 4.5 Experiments are first-class
An **experiment spec** is a frozen, hashed document: config, rule-set versions, seeds, sweep design, intervention schedule, null-model arms, and the **analysis plan** (metrics and detectors fixed before outcomes are seen). A run manifest records code version, rule-set version, full config, initial conditions, seed and stream derivation, environment, intervention schedule and analysis version. No silent defaults: a missing parameter is an error.

Counterfactual forks come almost free from determinism: checkpoint, then rerun with a member removed or an intervention applied, using the same random keys (common random numbers).

## 5. Measurement

Measures are separate, decomposable families; no composite score in v1.

| Construct | Operational definition | Notes / confound |
|---|---|---|
| Coalition (detected) | Set of entities with persistent mutual **flux** over a window above a null threshold; detector chosen before outcomes | Co-location alone is aggregation, not organization; spatial-shuffle null required |
| Interdependence | Drop in a member's viability when a specific partner is removed (counterfactual fork) | |
| Synergy | Super-additivity: collective output exceeds the sum of members' outputs when each is run alone under the same conditions (solo counterfactuals) | Ablation loss alone shows dependence, not synergy; additive production also shows ablation loss |
| Specialization | Between-member variation in realized flux profiles within a coalition, relative to a within-lineage baseline | Distinct from genotype diversity |
| Encapsulation | Fraction of a coalition's resource flux that stays among members (in vs out) | Needs a detector-independent check |
| Selection-level shift (for RQ1b) | Price-equation partition of fitness change into within- and between-group components [E for the equation; its interpretation is debated] | |
| Individuality (cross-check) | Information-theoretic measures (e.g. Krakauer-style [U]) | Partial information decomposition has competing definitions [S]; cross-check only |
| Persistence / resilience | Coalition lifetime; recovery after scheduled perturbation | |
| Emergence | Defined, not observed: a macro-variable M is "emergent" only if it exceeds its distribution in the neutral and no-interaction null worlds, and the generating micro-mechanism is specified in the config | Never "because it was surprising" |

**Null and control worlds** (run in every experiment): N1 interaction channel off; N2 well-mixed; N3 selection off (neutral drift, same demography); N4 spatially shuffled detector input; N5 genome frozen (no mutation).
**Construct check (from the prior project's failure):** before a registered run, show each primary outcome (i) is not constant across arms by construction, (ii) is near zero in N1/N3, (iii) responds in a designed positive-control world.

## 6. Glossary (intended meanings; never interchangeable)

| Term | Meaning here |
|---|---|
| Entity | A body with integer internal stocks, a genome, a location |
| Resource | A conserved integer quantity of a typed substance |
| Flux | Resource transferred per tick between sites/entities/environment |
| Dissipation | Resource removed from the closed accounting as waste (a sink, not "entropy") |
| Genotype / phenotype | Gene list vs realized per-tick behaviour (uptake, secretion, etc.) |
| Mutation frequency / magnitude / structural | Probability per gene per reproduction / size of change / gene deletion, duplication, insertion |
| Fitness | Realized offspring count (an outcome, not a built-in score) |
| Adaptation | Heritable change in the population increasing fitness in a given environment (no within-life learning in v1) |
| Cooperation | Actor pays a cost, recipient gains a benefit |
| Synergy | Super-additivity of collective output over solo outputs |
| Interdependence / obligate | Viability loss on removal of a partner / the entity cannot persist without it |
| Aggregation | Spatial proximity only; **not** organization |
| Coalition | A detected persistent flux-connected group of separate bodies |
| Encapsulation | Share of group flux internal to the group |
| Individuality | Selection acting on the group as a unit (heritable variation in group fitness) |
| Organization | A pattern in which the interaction structure and member roles remain after controlling for null worlds |
| Emergence | See §5; not unpredictability |
| Novelty vs complexity | New phenotype not seen before vs number/diversity of parts or interactions; a system can have one without the other |
| Null world | A configuration that removes a named mechanism, used as baseline |
| Construct check | Test that a measure can actually fail to show the effect |

## 7. Technology recommendation

| Option | Control | Speed (no GPU) | Cross-machine determinism | Debug / iterate | Verdict |
|---|---|---|---|---|---|
| **Rust kernel + Python (PyO3/maturin) lab + browser UI** | high | high | high (integer state, pinned toolchain) | medium (compile cycle) | **Recommended** |
| Python + NumPy/Numba | medium | medium; Numba ties results to LLVM/CPU behaviour | weak guarantees | fast | Good for analysis only |
| Julia | high | high | needs separate effort; not installed | fast | Rejected: extra runtime for the owner, no advantage here |
| Go | medium | medium | good | medium | Rejected: weak numerics and Python/data ecosystem |

**Choice [M]:** Rust kernel (`rayon` for parallelism, deterministic by the §4.4 design), Python for experiments and analysis (NumPy, Parquet/Arrow storage), a local server (axum or similar, WebSocket) plus a TypeScript browser frontend (Canvas/WebGL) for the live view and visual world editor. Configs are **TOML** validated by a JSON schema (YAML's implicit typing is a reproducibility hazard). The editor emits the same config the CLI consumes; the config is canonical.
Reasons: reproducibility and speed on a laptop, and the existing toolchain is installed (Rust 1.98, Node 26, Python 3.14). NumPy is not yet installed; I'll use a virtualenv.
Costs: slower rule iteration than Python, and a UI is real work. Mitigation: M1 viewer is read-only and small; the editor is M3.
*Unmeasured:* I will not assume throughput. A first-week benchmark (entities × ticks per second at a realistic gene count, single vs 12 threads, with thermal throttling observed) decides whether the plan is feasible on this machine. Disk (37 GB) means summaries and sampled snapshots, not full trajectories, by default.

## 8. Proposed decision records (append-only, one file `docs/DECISIONS.md`)

Style: ≤ 10 lines each; Status/Supersedes only; I draft, owner approves; superseded, never edited. Soft cap ≤ 15 through M2.

| # | One-line decision |
|---|---|
| P-1 | Technology: Rust kernel, Python lab, browser UI, TOML config (§7) |
| P-2 | Kernel state is integer/fixed-point only; floats are analysis-only (§4.4) |
| P-3 | Counter-based keyed RNG; stream key = (seed, stream, tick, entity, purpose) |
| P-4 | Strict integer conservation with explicit influx/dissipation (§4.3) |
| P-5 | Core has no group primitive; groups are detected post hoc (Q6) |
| P-6 | Genome = typed genes with maintenance cost; independent mutation operators (§4.2) |
| P-7 | Every experiment ships null worlds N1–N5 and a construct check before registered runs (§5) |
| P-8 | Experiment spec is frozen and hashed with the analysis plan before outcomes are seen |
| P-9 | Synergy = super-additivity vs solo counterfactuals; ablation alone is not synergy (§5) |
| P-10 | Decision-record policy: single file, soft cap, supersede-only (§2, item 3) |
| P-11 | Entity model reserves a `parent` containment slot now; fusion is implemented in M5 |
| P-12 | Evolutionary adaptation only in v1; plasticity is a later ablatable module |

## 9. Milestones (each ends with run output shown, not summarized)

| M | Content | Done when (verifiable) |
|---|---|---|
| 0 | **This proposal approved.** Literature pass for items marked [U]; benchmark spike (throwaway) | You approve; benchmark numbers on this machine reported; literature notes written |
| 1 | Deterministic core: lattice, integer resources, RNG streams, checkpoint/replay, state hash; read-only live viewer | Property tests pass (thread-count invariance, conservation, stream isolation, viewer-attached ≡ detached); replay hash matches on a second platform if available |
| 2 | Genome, mutation, reproduction/death, exchange + signal; experiment runner with manifests; null worlds N1–N5 | Neutral-drift sanity checks behave as theory predicts; manifests sufficient to replay a run from scratch |
| 3 | Visual world editor; perturbation schedules | A world authored in the UI replays identically from its saved config |
| 4 | Group detector, ablation/solo-counterfactual synergy, Price decomposition; construct check; **first RQ1 experiment**, designed and frozen before running | Construct check passes; results reported as distributions over ≥ N seeds with the null arms |
| 5 | Containment/fusion module (RQ1b) | Conservation holds across fusion events; new property tests; separate research question named |

## 10. Risks that could invalidate RQ1 results

1. Spatial structure alone, not interdependence, may explain persistence → N2 and a spatial-scale sweep.
2. Imposed dependence is nearly tautological → RQ1-evolved is the primary claim; RQ1-imposed is a baseline.
3. Detector choice creates or hides coalitions → two independent detectors; agreement reported; spatial-shuffle null.
4. Gene bloat or loss driven by cost parameters, not mechanism → sweep maintenance cost; report sensitivity.
5. Garden of forking paths → frozen analysis plans (P-8).
6. Free-rider collapse might be an artifact of discrete synchronous updates → check against a randomized-order-within-tick variant.
7. Replication on "someone else's system" is only as good as the platforms I can actually test.

## 11. Literature pass (before relying on any [U] item)
Targets: Black Queen hypothesis (Morris et al.), major transitions in evolution (Maynard Smith & Szathmáry; Michod), multilevel selection and the Price equation (Okasha), information theory of individuality (Krakauer et al.), partial information decomposition (Williams & Beer, and critiques), chemical organization theory and "arrival of the fittest" (Fontana & Buss), prior artificial-life platforms (Tierra, Avida, Lenia, Polyworld) for what they already showed about cross-feeding and cooperation, counter-based RNG (Philox, Salmon et al.). Any novelty claim in a document waits for this pass (rule 9).

## 12. Open items for the owner
1. Approve §3 scope, §6 glossary, §7 technology, §8 proposed records (all, or list changes).
2. Confirm two overrides I made: fusion deferred to M5 not v1 (P-11), and plasticity out of v1 (P-12).
3. **Answered 2026-11-16:** no second machine; owner asked me to set up CI. Plan: GitHub Actions matrix (Linux, macOS, Windows runners) replaying a reference run and comparing state hashes. Needs a GitHub remote, which the owner must create or authorize; pushing is not done without explicit approval. Added to M1 "done when".
4. **Answered 2026-11-16:** audience = theory-building with publication-grade rigor.
5. Approval of §3, §6, §7, §8 is still pending (owner has not yet said "approved").
