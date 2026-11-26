# Bug-sweep handoff

For a reviewer who has not seen the build. Read `CLAUDE.md`, then `docs/ODD.md` (what the model is
meant to do), `docs/TESTING.md` (what has been checked) and `docs/DECISIONS.md`. The author of the
code wrote the tests and nobody else has reviewed either.

## Run everything

```
cargo build --release
cargo test --workspace --release
cargo clippy --workspace --all-targets -- -D warnings
PYTHONPATH=python python3 -m unittest discover -s python/tests
PYTHONPATH=python python3 -m evolab run experiments/demo.toml --out results --jobs 4 --exploratory
./target/release/lab studio --port 8787
```

CI (`.github/workflows/replay.yml`) repeats the checks on Linux, macOS and Windows and replays each
`configs/<name>.sha256` at 1 and 4 threads. A changed rule or config must change its pin on purpose.

## Invariants (a violation is a bug)

1. Same config and seed give the same state hash at any thread count, on any platform, whether or not
   the run is observed or watched, and whether or not it was resumed from a checkpoint.
2. Conservation, per resource, after every tick: `total + dissipated + converted_out = initial + influx + converted_in`.
3. No floating point in `lab-core` or `lab-rules`; no hash-ordered iteration.
4. One rule's random draws never depend on which other rules exist.
5. Containment is one level; a host is alive, free and on its guest's site.
6. A config has no silent defaults; an unknown or missing setting is an error.
7. Observation never changes state; its totals reconcile with the ledgers for what it records.

## Riskiest code, most to least

| Area | Where | What to attack |
|---|---|---|
| Apply semantics | `crates/lab-core/src/apply.rs` | Order of transfers, conversions with leaks, moves with guests, attach/detach, deaths and births in one step; index validity after removals; overflow paths |
| Analysis measures | `python/evolab/{capability,flux,assortment,price,counterfactual,metrics}.py` | Any measure that is fixed by construction in some world (this already happened once, below); empty populations; cycles in the chemistry |
| Containment | `crates/lab-rules/src/containment.rs`, `lifecycle.rs` (`Room`) | Capacity with guests, partition after births, binding two bodies in one step |
| Checkpoint decoder | `crates/lab-core/src/codec.rs` | Hostile or truncated input (hand-written cases only; fuzz it) |
| Studio server | `crates/lab-cli/src/studio/`, `edit.rs`, `live.rs` | Path and name checks, Host and Content-Type checks, requests during run replacement, large worlds, slow clients (one request thread) |
| Observer | `crates/lab-core/src/observe.rs`, `crates/lab-cli/src/observe.rs` | Windows at the end of a run, host-guest exchange |
| Experiment runner | `python/evolab/{runner,spec,transforms,calibrate,stats,registered}.py` | Frozen-spec checks that can be bypassed; partial reruns into an old results folder |

## Decisions taken on the owner's behalf

Records DR-014 to DR-024 and the rulings in `docs/plans/2026-11-26-completion.md`. The ones most worth
challenging:

| Ruling | Cost if wrong |
|---|---|
| Older entities take precedence when a site is nearly full at division | A hidden age advantage shapes selection |
| Division spends half of each threshold; end products decay on sites (DR-024); values not tuned | Dependence results are specific to these numbers |
| "Dependence" = needs and takes up a product it cannot build from outside supply; sensing gates ignored; level 0 means absent | Gated or very weak genes are misclassified |
| Flux attribution splits uptake among nearby releasers in proportion to release | Who-fed-whom is wrong when several donors mix |
| Group detectors failed their check and are not outcomes | None, unless someone reads those columns as results |
| One level of containment; `site_capacity` also bounds guests per host | Limits what containment can become |
| Neutral null matched on population size and death rate only | N3 differs in age structure and crowding |

## Faults already found after tests passed (look for relatives)

- The dependence measure read zero **by construction** in the symmetric world, and a never-consumed
  resource piled up until nobody had to make it. Found by inspecting final genomes, not by tests.
- The command-line report table rounded every fraction to 0. Found by following the README.
- An experiment spec had been committed empty. Found by loading every spec.
- Arm and sweep names could place results outside the results folder.
- A report test used a folder name Windows forbids. Found by CI.
- Studio charts could stop short of the last tick (redraw throttle dropped the final draw). Found in Firefox.
- Analysing a well-mixed world needed over 9 GB per run (43.5 million donor-taker pairs in one window);
  the out-of-memory killer ended a sweep. `max_flux_pairs` now bounds it. Found by running the sweep.

## Known gaps

- An experiment started in the studio keeps running if the studio is closed.
- An interrupted experiment cannot resume; rerun it (results are deterministic).
- A run that fails analysis fails the whole experiment after all runs finish.
- Large analyses hold a whole observation window in memory. If a worker process dies, the runner
  reports only `BrokenProcessPool`, and finished runs are not collected.
- Studio map shows metabolic types and hosts, not detected groups (they failed their check).
- Not tested: Safari, macOS and Windows browsers, small screens, screen readers. No automated browser tests.
- Not tested: worlds far larger than 512 × 512 or genomes near the 4096-gene limit.
- Performance measured on one laptop.
