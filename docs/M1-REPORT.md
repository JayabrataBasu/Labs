# M1 report — deterministic core

Date: 2026-11-16. Plan: `docs/plans/2026-11-16-m1-deterministic-core.md`. Branch `m1-core`.
Status tags: **[V]** verified by output below, **[U]** not verified.

## 1. What exists

Four crates: `lab-rng` (Philox4x32-10), `lab-core` (state, lattice, canonical codec + SHA-256 hash, transfer
deltas, rule trait, scheduler, config), `lab-rules` (influx, diffusion, uptake, maintenance, movement),
`lab-cli` (`lab run | resume | hash | view`). No genomes, reproduction or death yet (M2).

## 2. Verified [V]

**Tests: 59 passed, 0 failed** (`cargo test --workspace --release`; clippy clean with `-D warnings`).

| Suite | Passed |
|---|---|
| lab-rng `kat.rs` (official Random123 vectors, counter layout) | 6 |
| lab-core `codec.rs` (lattice, round-trip, hash sensitivity, malformed checkpoints) | 8 |
| lab-core `delta.rs` (transfers, clipping, ledger, overflow, moves) | 8 |
| lab-core `sched.rs` (thread invariance at 1/2/5/12, rule order, stream checks, tick limit) | 7 |
| lab-rules `rules.rs` (each rule by hand-computed values, config errors, no-float scan) | 14 |
| lab-cli `cli.rs` (replay, threads, resume, manifest, refusals) | 8 |
| lab-cli `properties.rs` (64 random worlds per property) | 5 |
| lab-cli `view.rs` + unit (viewer attached ≡ detached, port busy, loopback only) | 3 |

**The property tests can fail.** The kernel was deliberately broken five ways; each property caught its bug, then the source was restored:

```
prop_thread_invariance: FAILED as required               (RNG made to depend on worker thread)
prop_conservation: FAILED as required                    (transfer credits one unit too many)
prop_stream_isolation: FAILED as required                (stream taken from rule position)
prop_resume_equivalence: FAILED as required              (checkpoint loader zeroes one stock)
prop_entities_sorted_and_parent_none: FAILED as required (move sets the reserved parent slot)
```

**Replay on this machine** (reference config, 2,000 ticks):
```
threads 4: final tick=2000 hash=8729cb309fc4fb5d2cfd14a1ec2d0b7da36c5b5a907c8eb74d0c86738d329191
threads 1: final tick=2000 hash=8729cb309fc4fb5d2cfd14a1ec2d0b7da36c5b5a907c8eb74d0c86738d329191
```
Pinned in `configs/reference.sha256`.

**Viewer:** run for 3,000 ticks and opened in headless Chromium; screenshot `docs/img/m1-viewer.png` shows the
heat-map, entities, controls and final hash. A test pauses, single-steps and polls a run and gets the same hash as a headless run.

**Official RNG vectors:** the three `philox4x32 10` lines of Random123's `kat_vectors` were fetched and match the M0 values.

## 3. Benchmark [V] (i5-1335U, release build)

512×512 sites, 1,048,576 entities, 5 rules, 100 ticks:

| Threads | Time | Entity-ticks/s |
|---|---|---|
| 1 | 28.3 s | 3.7 × 10⁶ |
| 4 | 15.5 s | 6.8 × 10⁶ |
| 12 | 16.5 s | 6.4 × 10⁶ |

Sustained load, 12 threads, 37 consecutive runs over 600 s: 5.4 × 10⁶ in the first run, 6.4–6.0 × 10⁶ in the
first minute, 6.8–7.0 × 10⁶ in the last minute (min 5.4 × 10⁶, max 7.0 × 10⁶). **No thermal slowdown observed.**

Reference world (4,096 sites, 2,000 entities): 2,000 ticks in 1.5 s.

**Finding:** the kernel is about 7× slower than the M0 spike at 12 threads (4.5 × 10⁷) and stops scaling after
about 4 threads. Proposals are parallel but application is single-threaded, and the site index is rebuilt for
every rule. This was the plan's stated risk. Results are unaffected; only speed is.
Consequence: 10⁴ entities × 10⁵ ticks on a 64×64 world is roughly 1–2 minutes per run, so a 1,000-run sweep is
on the order of a day, not the 6 hours estimated in M0. Usable for M2; a real constraint for M4 sweeps.

## 4. Not verified [U]

- **Cross-platform replay.** The CI workflow (`.github/workflows/replay.yml`: Linux, macOS arm64, Windows) is
  written but had not run when this report was written. **Update:** run 43915903083 passed on all three with hash
  `8729cb30…329191` at 1 and 4 threads, including the LF line-ending handling; the M2 pins are in M2-M3-REPORT.
- Line endings: the config hash covers the exact bytes of the config file, so `.gitattributes` forces LF. Whether
  that is sufficient on the Windows runner is part of the CI check.
- The viewer was checked in headless Chromium only, not by a person and not in Firefox.
- Runs longer than 600 s; worlds larger than 512×512.

## 5. Known limits

- `git_commit`/`git_dirty` in a manifest describe the binary at build time. A registered experiment must be run from a clean, committed build.
- Seeds are limited to 0…2⁶³−1 (TOML integers).
- `lab view` starts from tick 0 only; it cannot resume a checkpoint.
- If a tick fails mid-way the in-memory state is discarded; the last checkpoint is the recovery point.
- The reference world is not at steady state (resource A accumulates); it is a replay fixture, not a model.

## 6. Proposed decision (not yet approved)

**DR-014 (proposed):** apply deltas in parallel by partitioning sites into fixed, thread-independent blocks,
with cross-block transfers applied in a second ordered pass; build the site index once per tick unless a rule
moves entities. Acceptance: all existing tests and the pinned reference hash unchanged, and a measured speed-up.
Owner direction of 2026-11-16 ("modify the plan if speed becomes really concerning") applies; I recommend
doing this at the start of M2, before reproduction makes the entity table dynamic.

## 7. Deviations from the plan (rulings)

- Ruling: work on branch m1-core in the main checkout (no worktree) — repo is new and single-user — cost if wrong: none, branch is isolated from master.
- Ruling: commit locally per task as the owner (git identity is theirs), no trailers, no push until owner asks — owner said "commit and push but with me as the author" and "working product is all good" — cost if wrong: local commits can be squashed/reset.
- Task 2: Ruling: added State::new(topo,k,site_stock,entities,ent_stock) constructor and validate() (not in plan's interface list) — tests and checkpoint loading need one validated way to build a state — cost if wrong: none, additive.
- Task 7: Ruling: property worlds enlarged to 3..=40 per side (plan said 3..=12) so they span several 1024-site chunks; a single-chunk world cannot expose chunk-order bugs — cost if wrong: slightly slower tests.
- Task 7: Ruling: "watch each fail against a wrong stub" done as mutation checks (kernel deliberately broken 5 ways; each property FAILED, then source restored) because the kernel already existed — cost if wrong: none; evidence is in the M1 report.
- Task 8: Ruling: added required flag --linger-seconds and optional flag --start-paused to `lab view` (not in plan) — the process must exit for tests and a human needs time to look at the final state; --start-paused lets a test attach before tick 1 — cost if wrong: two extra flags.

## 8. Full test output

```
     Running unittests src/main.rs (target/release/deps/lab-b9dd8e115f2104ea)

running 1 test
test view::tests::binds_loopback_only ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/cli.rs (target/release/deps/cli-b2414b5e5ebf5183)

running 8 tests
test bad_config_reports_the_problem ... ok
test too_many_ticks_rejected ... ok
test resume_rejects_other_config ... ok
test manifest_complete ... ok
test checkpoint_every_writes_intermediate_files ... ok
test run_twice_same_hash ... ok
test threads_do_not_change_hash ... ok
test resume_equals_straight_run ... ok

test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.33s

     Running tests/properties.rs (target/release/deps/properties-623c19af72dd9f88)

running 5 tests
test prop_entities_sorted_and_parent_none ... ok
test prop_conservation ... ok
test prop_stream_isolation ... ok
test prop_resume_equivalence ... ok
test prop_thread_invariance ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.32s

     Running tests/view.rs (target/release/deps/view-bbbf87c999deb454)

running 2 tests
test port_in_use_is_clean_error ... ok
test attached_equals_detached ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.41s

     Running unittests src/lib.rs (target/release/deps/lab_core-04ee53e37a416277)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/codec.rs (target/release/deps/codec-78a7631125b955d9)

running 8 tests
test decode_rejects ... ok
test new_computes_totals_and_next_id ... ok
test lattice_neighbours ... ok
test lattice_rejects_degenerate ... ok
test roundtrip ... ok
test hash_changes_on_any_field ... ok
test site_index_groups ... ok
test new_rejects_inconsistent_input ... ok

test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/delta.rs (target/release/deps/delta-d2f1baf4e05d4b36)

running 8 tests
test conservation_detects_tamper ... ok
test invalid_holders_rejected ... ok
test order_matters_and_is_slice_order ... ok
test move_changes_site ... ok
test overflow_is_error ... ok
test source_and_sink_update_ledger ... ok
test transfer_clips ... ok
test transfer_moves_exact ... ok

test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/sched.rs (target/release/deps/sched-75b98148ebc4cd4f)

running 7 tests
test duplicate_or_reserved_stream_rejected ... ok
test tick_limit_is_an_error ... ok
test zero_entities_runs ... ok
test rule_order_is_config_order ... ok
test seed_sensitivity ... ok
test ledger_matches_totals ... ok
test thread_invariance ... ok

test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.08s

     Running unittests src/lib.rs (target/release/deps/lab_rng-b2f86658c95dcaa5)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/kat.rs (target/release/deps/kat-dc931a90de80fb46)

running 6 tests
test draw_layout ... ok
test below_bounds ... ok
test kat_ones ... ok
test kat_pi ... ok
test streams_differ ... ok
test kat_zero ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/lib.rs (target/release/deps/lab_rules-b866add079e27061)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/rules.rs (target/release/deps/rules-f82762e38bfaa262)

running 14 tests
test config_hash_is_of_exact_text ... ok
test influx_adds_on_schedule ... ok
test parse_rejects_bad_values ... ok
test maintenance_dissipates_and_clips ... ok
test parse_rejects_missing_field ... ok
test initial_placement_is_seeded_and_complete ... ok
test diffusion_shares_by_integer_division ... ok
test move_on_isolated_site_is_a_no_op ... ok
test no_floats_in_kernel ... ok
test parse_rejects_unknown_field ... ok
test move_probability_extremes ... ok
test uptake_takes_requested_amount ... ok
test uptake_priority_is_not_id_order ... ok
test reference_config_parses_and_runs_10_ticks ... ok

test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

   Doc-tests lab_core

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests lab_rng

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests lab_rules

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

```
