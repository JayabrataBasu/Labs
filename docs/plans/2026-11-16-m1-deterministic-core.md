# M1 Deterministic Core Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A deterministic, conserved, thread-count-invariant simulation kernel with checkpoint/replay, a read-only browser viewer, and a CI job that proves the same config gives the same state hash on Linux, macOS and Windows.

**Architecture:** A Cargo workspace of four crates. Each tick runs the configured rules in order; a rule reads an immutable state and *proposes* deltas in parallel over fixed-size site chunks; deltas are concatenated in chunk order and applied sequentially. Every resource change is a `Transfer` between holders (site, entity, source, sink), so conservation holds by construction and is also checked exactly. One canonical little-endian serialization serves as both checkpoint format and hash input.

**Tech Stack:** Rust 1.98.0 (pinned), `rayon`, `serde`, `toml`, `serde_json`, `sha2`, `clap`, `tiny_http`, `proptest` (dev). Viewer is one static HTML file with vanilla JS (no Node build in M1). No Python in M1.

**Spec:** `docs/PROPOSAL.md` (§4, §9 row M1), `docs/DECISIONS.md` (DR-001…DR-013), `docs/M0-REPORT.md`.

## Global Constraints

- Research question served: RQ1 (PROPOSAL §3.2). M1 adds no evolutionary mechanism; it is the instrument's foundation.
- DR-002: no `f32`/`f64` in `lab-rng`, `lab-core`, `lab-rules`. Each has `#![deny(clippy::float_arithmetic)]` and `#![forbid(unsafe_code)]`; a test greps their `src/` for `f32|f64` and fails on a match.
- No `HashMap`/`HashSet` in those crates (`clippy.toml` `disallowed-types`). Use `Vec`/`BTreeMap`.
- DR-003: RNG is Philox4x32-10 only; key = seed (u64), counter = (tick u32, subject u64, stream u16, draw index u16). No other randomness source.
- DR-004: all quantities `u64`; all arithmetic on stocks and ledgers is `checked_*`; overflow is an `Err`, never a wrap or panic.
- DR-005: no group concept anywhere in the kernel. DR-011: `EntityMeta.parent: Option<u64>` exists and is always `None` in M1.
- No silent defaults: every config struct uses `#[serde(deny_unknown_fields)]` and has no `Option`/`default` fields.
- Results must not depend on thread count or on whether the viewer is attached.
- `rust-toolchain.toml`: `channel = "1.98.0"`. `Cargo.lock` is committed. No `target-cpu=native`, no fast-math.
- Git: the owner is the sole author. No attribution trailers. Commit only when the owner has authorized it; otherwise stop at each "Commit" step and report.
- Engine code must not copy from `~/firma`.

## Review Focus

1. **Config with a missing or misspelled field** → a clear error naming the field; never a default. (Task 5)
2. **Contention bias:** several entities wanting the same scarce resource must not be served in id order (id correlates with age). Order by keyed random priority. (Task 5)
3. **Overflow:** very large influx or stock values → `Err(Overflow)`, not wraparound that would silently break conservation. (Task 3)
4. **Resuming a checkpoint with a different config or format version** → refused with an error, not a silently different run. (Task 6)
5. **Degenerate worlds:** zero entities must run; a wrapped lattice with width or height < 3 (duplicate neighbours) and a run longer than 2³²−1 ticks must be rejected at validation. (Tasks 2, 5, 6)

## File Structure

```
Cargo.toml, rust-toolchain.toml, clippy.toml, .github/workflows/replay.yml
crates/lab-rng/src/lib.rs            Philox, RuleRng, below()
crates/lab-core/src/
  lib.rs        re-exports
  state.rs      State, EntityMeta, Ledger, site index
  topology.rs   Lattice → CSR adjacency
  codec.rs      canonical bytes, checkpoint, SHA-256 state hash
  delta.rs      Holder, Delta, apply, conservation check
  rule.rs       Rule trait
  sched.rs      tick loop, chunked parallel propose
  config.rs     Config structs, validation
  error.rs      LabError
crates/lab-rules/src/   lib.rs (registry), influx.rs, diffusion.rs, uptake.rs, maintenance.rs, movement.rs
crates/lab-cli/src/     main.rs (clap), run.rs (run/resume/manifest), view.rs (server), viewer.html
configs/reference.toml, configs/reference.sha256
tests/ live inside each crate (`crates/*/tests/`)
docs/M1-REPORT.md
```

---

### Task 1: Workspace and `lab-rng`

**Files:** Create `Cargo.toml`, `rust-toolchain.toml`, `clippy.toml`, `crates/lab-rng/{Cargo.toml,src/lib.rs,tests/kat.rs}`.

**Interfaces — Produces:**
```rust
pub fn philox4x32_10(ctr: [u32; 4], key: [u32; 2]) -> [u32; 4];
#[derive(Clone, Copy)] pub struct RuleRng { pub seed: u64, pub stream: u16 }
impl RuleRng {
    /// ctr = [tick, subject as u32, (subject >> 32) as u32, (stream as u32) << 16 | index as u32]; key = [seed as u32, (seed >> 32) as u32]
    pub fn draw(&self, tick: u32, subject: u64, index: u16) -> [u32; 4];
}
/// Unbiased-enough integer scaling without floats: ((x as u64 * n as u64) >> 32) as u32. Panics if n == 0.
pub fn below(x: u32, n: u32) -> u32;
pub const STREAM_INIT: u16 = 0; // reserved for initial placement
```

- [ ] **Step 1:** Fetch the official vectors: `https://raw.githubusercontent.com/DEShawResearch/random123/main/tests/kat_vectors`, take the three `philox4x32 10` lines, and put that URL in a comment in `tests/kat.rs`. If the file disagrees with the M0 values, the file wins; report it.
- [ ] **Step 2: Failing tests** in `tests/kat.rs`:
```rust
#[test] fn kat_zero() { assert_eq!(philox4x32_10([0;4],[0;2]), [0x6627e8d5,0xe169c58d,0xbc57ac4c,0x9b00dbd8]); }
#[test] fn kat_ones() { assert_eq!(philox4x32_10([u32::MAX;4],[u32::MAX;2]), [0x408f276d,0x41c83b0e,0xa20bc7c6,0x6d5451fd]); }
#[test] fn kat_pi()   { assert_eq!(philox4x32_10([0x243f6a88,0x85a308d3,0x13198a2e,0x03707344],[0xa4093822,0x299f31d0]), [0xd16cfe09,0x94fdcceb,0x5001e420,0x24126ea1]); }
#[test] fn draw_layout() { let r = RuleRng{seed:0x1122334455667788,stream:7};
    assert_eq!(r.draw(9, 0xAABBCCDD00112233, 5), philox4x32_10([9,0x00112233,0xAABBCCDD,(7<<16)|5],[0x55667788,0x11223344])); }
#[test] fn below_bounds() { assert_eq!(below(0,10),0); assert_eq!(below(u32::MAX,10),9); assert_eq!(below(1<<31,10),5); }
#[test] fn streams_differ() { let a=RuleRng{seed:1,stream:1}; let b=RuleRng{seed:1,stream:2}; assert_ne!(a.draw(0,0,0), b.draw(0,0,0)); }
```
- [ ] **Step 3:** Run `cargo test -p lab-rng`; expect compile failure.
- [ ] **Step 4:** Implement. Philox constants: multipliers `0xD2511F53`, `0xCD9E8D57`; Weyl `0x9E3779B9`, `0xBB67AE85`; 10 rounds, key bumped before rounds 2–10 (reference: `spikes/m0-bench/bench.rs`, which passed these vectors).
- [ ] **Step 5:** `cargo test -p lab-rng` → 6 passed. `cargo clippy -p lab-rng -- -D warnings` clean.
- [ ] **Step 6:** Commit: "M1: workspace and Philox RNG crate".

### Task 2: State, topology, canonical codec and hash

**Files:** Create `crates/lab-core/{Cargo.toml,src/{lib,state,topology,codec,error}.rs,tests/codec.rs}`.

**Interfaces — Produces:**
```rust
pub enum LabError { Config(String), Overflow(&'static str), Conservation{resource: usize, expected: u64, actual: u64}, Codec(String), Io(String) }
pub struct Topology { pub width: u32, pub height: u32, pub wrap: bool, /* CSR */ }
impl Topology {
    pub fn lattice(width: u32, height: u32, wrap: bool) -> Result<Self, LabError>; // Err if width==0||height==0, or wrap && (width<3||height<3)
    pub fn n_sites(&self) -> u32;
    pub fn neighbours(&self, site: u32) -> &[u32]; // order N,E,S,W; non-wrapping edges simply omit missing neighbours
}
pub struct EntityMeta { pub id: u64, pub site: u32, pub parent: Option<u64> }
pub struct Ledger { pub influx: Vec<u64>, pub dissipated: Vec<u64> } // cumulative, per resource
pub struct State {
    pub tick: u64, pub k: usize, pub topo: Topology,
    pub site_stock: Vec<u64>,          // n_sites * k, index site*k + r
    pub entities: Vec<EntityMeta>,     // always sorted by id
    pub ent_stock: Vec<u64>,           // entities.len() * k, aligned with `entities`
    pub ledger: Ledger, pub next_entity_id: u64, pub initial_total: Vec<u64>,
}
impl State {
    pub fn total(&self, r: usize) -> Result<u64, LabError>;       // sites + entities, checked
    pub fn site_index(&self) -> SiteIndex;                         // entity indices grouped by site, ascending id within site
}
pub struct SiteIndex; impl SiteIndex { pub fn at(&self, site: u32) -> &[u32]; }
pub const FORMAT_VERSION: u32 = 1;
pub fn encode(state: &State, config_hash: &[u8; 32]) -> Vec<u8>;  // magic b"LABS", FORMAT_VERSION, config_hash, then every State field, all integers little-endian
pub fn decode(bytes: &[u8]) -> Result<(State, [u8; 32]), LabError>; // Err on bad magic, version mismatch, truncated or trailing bytes
pub fn state_hash(state: &State, config_hash: &[u8; 32]) -> [u8; 32]; // SHA-256 of encode()
```

- [ ] **Step 1: Failing tests** (`tests/codec.rs`): `lattice_neighbours` (3×3 wrap: site 0 → `[6,1,3,2]`; 3×3 no-wrap: site 0 → `[1,3]`, site 4 → `[1,5,7,3]`); `lattice_rejects_degenerate` (`lattice(2,5,true)`, `lattice(0,5,false)` are `Err`; `lattice(1,1,false)` is `Ok` with no neighbours); `roundtrip` (`decode(encode(s,h)) == (s,h)` for a hand-built state with 2 entities, k=2); `hash_changes_on_any_field` (flipping tick, one site stock, one entity site, `parent`, ledger, or config hash each changes `state_hash`); `decode_rejects` (bad magic, version 2, truncated by one byte, one trailing byte → `Err(Codec)`); `site_index_groups` (entities at sites `[2,0,2]` → `at(2)` = `[0,2]`, `at(0)` = `[1]`, `at(1)` empty).
- [ ] **Step 2:** Run; expect failure. **Step 3:** Implement. **Step 4:** `cargo test -p lab-core` passes; clippy clean.
- [ ] **Step 5:** Commit: "M1: state, lattice topology, canonical codec and state hash".

### Task 3: Deltas, application, conservation

**Files:** Create `crates/lab-core/src/delta.rs`, `crates/lab-core/tests/delta.rs`.

**Interfaces — Produces:**
```rust
#[derive(Clone, Copy, PartialEq, Eq, Debug)] pub enum Holder { Site(u32), Entity(u32 /* index into State.entities, valid for this tick */), Source, Sink }
#[derive(Clone, Copy, PartialEq, Eq, Debug)] pub enum Delta {
    Transfer { from: Holder, to: Holder, resource: u16, amount: u64 },
    Move { entity: u32, to: u32 },
}
/// Applies in slice order. A Transfer is clipped to what `from` holds at that moment (Source is unlimited).
/// Source→x adds to ledger.influx; x→Sink adds to ledger.dissipated. Source→Sink, Sink as `from`, Source as `to` are Err(Config).
pub fn apply(state: &mut State, deltas: &[Delta]) -> Result<(), LabError>;
/// For every resource: total == initial_total + influx − dissipated, exactly.
pub fn check_conservation(state: &State) -> Result<(), LabError>;
```

- [ ] **Step 1: Failing tests:** `transfer_moves_exact` (site 10 → entity, amount 4: site 6, entity +4); `transfer_clips` (request 15 from a site holding 10 → moves 10; a following request gets 0); `order_matters_and_is_slice_order` (two entities each request 7 from a site holding 10 → first gets 7, second 3); `source_and_sink_update_ledger` (Source→Site 5 then Entity→Sink 2 → `influx[r]==5`, `dissipated[r]==2`, `check_conservation` is `Ok`); `conservation_detects_tamper` (manually add 1 to a site stock → `Err(Conservation{..})`); `overflow_is_error` (site at `u64::MAX`, Source→Site 1 → `Err(Overflow)`); `move_changes_site` (Move to a neighbour updates `entities[i].site`; Move to a non-neighbour or out-of-range site → `Err(Config)`); `invalid_holders_rejected`.
- [ ] **Step 2–4:** fail, implement, pass. **Step 5:** Commit: "M1: transfer deltas with exact conservation".

### Task 4: Rule trait and scheduler

**Files:** Create `crates/lab-core/src/{rule,sched}.rs`, `crates/lab-core/tests/sched.rs`.

**Interfaces — Produces:**
```rust
pub trait Rule: Send + Sync {
    fn id(&self) -> &'static str;      // e.g. "uptake.fixed"
    fn version(&self) -> u32;
    fn stream(&self) -> u16;           // unique per rule instance in a run; never STREAM_INIT
    /// Read-only. Push deltas for sites in `sites` only, in a deterministic order.
    fn propose(&self, state: &State, index: &SiteIndex, rng: RuleRng, sites: std::ops::Range<u32>, out: &mut Vec<Delta>);
}
pub const CHUNK_SITES: u32 = 1024; // fixed: chunking must not depend on thread count
pub struct Engine { /* rules, seed, rayon pool */ }
impl Engine {
    pub fn new(rules: Vec<Box<dyn Rule>>, seed: u64, threads: usize) -> Result<Self, LabError>; // Err on duplicate stream or stream == STREAM_INIT
    /// One tick: for each rule in order { build SiteIndex; propose over chunks in parallel; concat in chunk order; apply }. Then check_conservation; tick += 1. Err if tick would exceed u32::MAX.
    pub fn step(&self, state: &mut State) -> Result<(), LabError>;
}
```

- [ ] **Step 1: Failing tests** using two test-only rules defined in the test file (`Sprinkle`: Source→Site of `below(draw,3)` per site; `Shuffle`: each entity moves to neighbour `below(draw, deg)`): `thread_invariance` (64×64 lattice, 5,000 entities, 50 ticks at threads 1, 2, 5, 12 → identical `state_hash`); `seed_sensitivity` (seed 1 vs 2 differ); `rule_order_is_config_order` (swapping two rules changes the hash); `duplicate_stream_rejected`; `ledger_matches_totals` (after 50 ticks `step` has returned `Ok` every time and `total(r) == initial_total[r] + influx[r] − dissipated[r]`); `zero_entities_runs`.
- [ ] **Step 2–4:** fail, implement (rayon `par_iter` over chunk ranges with order-preserving `collect`), pass.
- [ ] **Step 5:** Commit: "M1: rule trait and deterministic parallel scheduler".

### Task 5: Config and the five M1 rules

**Files:** Create `crates/lab-core/src/config.rs`, `crates/lab-rules/{Cargo.toml,src/{lib,influx,diffusion,uptake,maintenance,movement}.rs,tests/rules.rs}`, `configs/reference.toml`.

**Interfaces — Produces:**
```rust
// lab-core::config — all fields required, deny_unknown_fields
pub struct Config { pub schema_version: u32 /* must be 1 */, pub seed: u64, pub world: WorldCfg, pub init: InitCfg, pub rules: Vec<RuleCfg> }
pub struct WorldCfg { pub width: u32, pub height: u32, pub wrap: bool, pub resources: Vec<String> /* 1..=16 unique names */ }
pub struct InitCfg { pub site_stock: Vec<u64>, pub entities: u64, pub entity_stock: Vec<u64> } // vectors must have length k
pub struct RuleCfg { pub id: String, pub version: u32, pub stream: u16, pub params: toml::Table }
pub fn parse(text: &str) -> Result<(Config, [u8; 32] /* SHA-256 of the exact text bytes */), LabError>;
pub fn initial_state(cfg: &Config) -> Result<State, LabError>; // entity i (id i) placed at site below(RuleRng{seed,stream:STREAM_INIT}.draw(0, i, 0)[0], n_sites)
// lab-rules
pub fn build(cfg: &Config) -> Result<Vec<Box<dyn Rule>>, LabError>; // Err on unknown id, version mismatch, bad params
```

Rule semantics (all version 1; resource named by string, resolved to index at build):

| id | params | behaviour per tick |
|---|---|---|
| `influx.constant` | `resource`, `amount`, `every` (≥1) | if `tick % every == 0`: Source→each site, `amount` |
| `diffusion.integer` | `resource`, `divisor` (≥5) | each site sends `stock / divisor` to each neighbour (computed from the pre-rule state; total out ≤ stock, so nothing clips) |
| `uptake.fixed` | `resource`, `amount` | each entity requests `amount` from its site. Within a site, requests are ordered by `draw(tick, id, 0)[0]` ascending, ties by id |
| `maintenance.fixed` | `resource`, `amount` | Entity→Sink `amount` (clipped) |
| `move.random` | `probability_ppm` (0..=1_000_000) | if `below(draw(tick,id,0)[0], 1_000_000) < ppm` and the site has neighbours: Move to neighbour `below(draw(tick,id,0)[1], deg)` |

`configs/reference.toml`: seed 20261116; 64×64 wrap; resources `["A","B"]`; `site_stock=[50,0]`, 2,000 entities, `entity_stock=[5,0]`; rules in order influx(A,2,every 1; stream 1), diffusion(A, divisor 8; stream 2), uptake(A,3; stream 3), maintenance(A,1; stream 4), move(250000 ppm; stream 5).

- [ ] **Step 1: Failing tests:** one behaviour test per rule on a 3×3 world with hand-computed expected stocks (e.g. diffusion: centre 80, divisor 8, no-wrap → centre 40, each of 4 neighbours 10); `uptake_priority_is_not_id_order` (site holding 3, 200 ticks, two entities requesting 3: each wins between 60 and 140 times); `move_probability_extremes` (ppm 0 → nobody moves in 20 ticks; ppm 1,000,000 → every entity moves every tick); `parse_rejects_unknown_field` (extra key `sed = 1` → error text contains `sed`); `parse_rejects_missing_field` (no `seed` → error text contains `seed`); `parse_rejects_bad_values` (schema_version 2, duplicate resource names, `site_stock` wrong length, duplicate stream, stream 0, unknown rule id, version 2, `divisor = 4`, `probability_ppm = 1000001`); `reference_config_parses_and_runs_10_ticks`; `no_floats_in_kernel` (reads `crates/lab-{rng,core,rules}/src/**/*.rs`, asserts no `f32`/`f64` token).
- [ ] **Step 2–4:** fail, implement, pass. **Step 5:** Commit: "M1: config schema and first rule set".

### Task 6: CLI — run, checkpoint, resume, manifest

**Files:** Create `crates/lab-cli/{Cargo.toml,build.rs,src/{main,run}.rs,tests/cli.rs}`.

**Interfaces — Produces:**
```
lab run <config.toml> --ticks <N> --threads <T> --out <dir> [--checkpoint-every <M>]
lab resume <checkpoint.bin> --config <config.toml> --ticks <N> --threads <T> --out <dir>
lab hash <checkpoint.bin>
```
`run` writes `<dir>/manifest.json` and `<dir>/ckpt-<tick>.bin` (always the final tick; also every M ticks) and prints one line `final tick=<t> hash=<64 hex>`.
`manifest.json` fields (all required): `lab_version`, `git_commit`, `git_dirty` (from `build.rs`), `rustc`, `target`, `format_version`, `config_text`, `config_sha256`, `seed`, `rng` = `"philox4x32-10; key=seed; ctr=(tick,subject,stream,index)"`, `rules` (id, version, stream), `threads`, `start_tick`, `end_tick`, `checkpoints` (tick, sha256), `final_sha256`, `entity_ticks_per_second`.
`resume` fails if the checkpoint's stored config hash differs from the given config, or on a format-version mismatch. `run`/`resume` fail before starting if `start_tick + ticks > u32::MAX`.

- [ ] **Step 1: Failing tests** (`tests/cli.rs`, invoking the built binary): `run_twice_same_hash`; `threads_do_not_change_hash` (1 vs 4 on the reference config, 200 ticks); `resume_equals_straight_run` (200 straight vs 100 + resume 100 → same final hash; `lab hash` on the final checkpoint prints the same value); `resume_rejects_other_config` (change `amount`, expect non-zero exit and message containing `config hash`); `manifest_complete` (every field above present and non-empty; `config_text` equals the file); `too_many_ticks_rejected`.
- [ ] **Step 2–4:** fail, implement, pass. **Step 5:** Commit: "M1: CLI with checkpoints, resume and run manifest".

### Task 7: Property tests for the core invariants

**Files:** Create `crates/lab-cli/tests/properties.rs` (uses `lab-core` + `lab-rules` as libraries; `proptest` with a fixed case count of 64).

- [ ] **Step 1:** Generator: width, height in 3..=12, wrap any, k in 1..=3, entities 0..=200, a random non-empty ordered subset of the five rules with valid params, seed any u64, ticks 1..=40.
- [ ] **Step 2: Properties** (write, watch each fail against a deliberately wrong stub once, then pass): `prop_thread_invariance` (threads 1 vs 3 vs 8 → same hash); `prop_conservation` (`check_conservation` is `Ok` after every tick, and `total(r) == initial_total[r] + influx[r] − dissipated[r]`); `prop_stream_isolation` (inserting a test-only rule that draws random numbers but emits no deltas, at any position with an unused stream, leaves the hash unchanged); `prop_resume_equivalence` (encode at a random midpoint, decode, continue → same hash as the straight run); `prop_entities_sorted_and_parent_none` (ids strictly ascending, every `parent` is `None`).
- [ ] **Step 3:** `cargo test --workspace` all pass; record the output for the M1 report. **Step 4:** Commit: "M1: property tests for determinism, conservation, stream isolation".

### Task 8: Read-only live viewer

**Files:** Create `crates/lab-cli/src/{view.rs,viewer.html}`, `crates/lab-cli/tests/view.rs`; modify `main.rs`.

**Interfaces — Produces:** `lab view <config.toml> --ticks <N> --threads <T> --port <P> --out <dir>` — binds `127.0.0.1:<P>` only; the simulation runs on its own thread and publishes a snapshot after each tick; HTTP handlers only read snapshots.
- `GET /` → `viewer.html` (embedded with `include_str!`).
- `GET /meta` → JSON `{width,height,resources,tick,ticks_total,done}`.
- `GET /frame?resource=<index>` → bytes: `tick u64 LE`, then per site `stock` saturated to `u32 LE`, then per site `entity_count u32 LE`.
- `POST /control` body `pause` | `resume` | `step` — affects wall-clock pacing only.
`viewer.html`: a canvas heat-map of the chosen resource with entity counts overlaid, a resource selector, pause/resume/step buttons, the tick counter, and the final hash when done. Polls `/frame` about 10×/s. No external scripts or network requests.
When finished, `view` writes the same manifest and checkpoint as `run`.

- [ ] **Step 1: Failing tests:** `attached_equals_detached` (run `lab view` on the reference config for 200 ticks while a test thread requests `/frame` and `/meta` continuously and sends `pause`/`step`/`resume`; final hash equals `lab run`'s); `frame_layout` (length is `8 + 8·n_sites`; leading tick parses; sum of entity counts is 2,000); `binds_loopback_only`; `port_in_use_is_clean_error`.
- [ ] **Step 2–4:** fail, implement, pass.
- [ ] **Step 5: Manual check:** `cargo run --release -p lab-cli -- view configs/reference.toml --ticks 5000 --threads 4 --port 8787 --out /tmp/…/view`, open `http://127.0.0.1:8787`, and capture a screenshot for the report. Say plainly if this could not be done.
- [ ] **Step 6:** Commit: "M1: read-only live viewer".

### Task 9: CI replay check, benchmark and M1 report

**Files:** Create `.github/workflows/replay.yml`, `configs/reference.sha256`, `docs/M1-REPORT.md`.

- [ ] **Step 1:** Produce the golden hash here: `lab run configs/reference.toml --ticks 2000 --threads 4 --out <scratch>`; write the 64-hex value to `configs/reference.sha256`. Confirm `--threads 1` gives the same.
- [ ] **Step 2:** Workflow: on push and pull_request to `master`; matrix `ubuntu-latest`, `macos-latest` (arm64), `windows-latest`; steps: checkout, install the pinned toolchain, `cargo clippy --workspace -- -D warnings`, `cargo test --workspace --release`, then run the reference config for 2,000 ticks at `--threads 1` and `--threads 4` and fail unless both printed hashes equal `configs/reference.sha256`.
- [ ] **Step 3:** Benchmark locally: reference world scaled to 512×512 with 1,048,576 entities for 100 ticks at threads 1, 4, 12 (compare with M0), plus one sustained 10-minute run at 12 threads reporting throughput in the first and last minute (thermal throttling).
- [ ] **Step 4:** Write `docs/M1-REPORT.md`: full `cargo test` output, benchmark tables, the viewer screenshot, what was verified, what was not, and the CI result per platform. A CI result can only be reported after the owner's push triggers it; until then the cross-platform claim is stated as untested.
- [ ] **Step 5:** Commit: "M1: CI replay check, reference hash and report". After the owner pushes, read the workflow result with `gh run list`/`gh run view` and add it to the report.

---

## Not in M1 (deliberately)

Genomes, reproduction, death, mutation, exchange between entities, signalling (M2); Python lab and experiment runner (M2); null worlds N1–N5 (M2); world editor and perturbation schedules (M3); metrics and group detection (M4); fusion (M5). Well-mixed topology arrives with the null worlds in M2, where it is first needed.

## Known limits of this plan

- **Owner direction (2026-11-16):** revise this plan if speed becomes a real problem for larger tasks; a working product is the priority. Trigger: if Task 9 shows sequential apply dominating run time at the scales M2–M4 need, propose a parallel apply (site-partitioned) as a new decision record before M2.
- Sequential delta application bounds parallel speed-up. This is accepted for M1 because it is the simplest scheme that is obviously order-independent of threads; Task 9 measures the cost. A parallel apply would be a new decision record.
- The viewer polls over HTTP instead of using WebSockets. Enough for a read-only view; M3 may revisit.
- macOS arm64 and Windows results depend on GitHub's hosted runners; "reproducible on any system" remains a claim limited to platforms actually tested.
