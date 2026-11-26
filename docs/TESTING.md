# Testing and evaluation log

Organised along the TRACE elements (Grimm et al. 2014). It says what has been checked, how, and what
has not. Run the checks with:

```
cargo test --workspace --release
cargo clippy --workspace --all-targets -- -D warnings
PYTHONPATH=python python3 -m unittest discover -s python/tests
```

## Problem formulation and model description

Question and scope: `docs/PROPOSAL.md` §3. Model: `docs/ODD.md`.

## Conceptual model

Assumptions that shape results, stated so they can be attacked: conservation with a yield below one;
costs per gene; leaks as a fixed share of product; division precedence by age; threshold sensing as
the only information channel; one level of containment.

## Implementation verification

| Property | How it is checked |
|---|---|
| Random generator is Philox4x32-10 | Known-answer vectors from the Random123 distribution (`crates/lab-rng/tests/kat.rs`) |
| Matter is conserved | Exact identity checked by the kernel after every tick; property test over random worlds |
| Result independent of thread count | Property test; CI replays pinned worlds at 1 and 4 threads |
| One rule's randomness cannot disturb another's | Property test inserting a rule that only draws numbers |
| Resume equals a straight run | Property test and command-line test |
| Observation changes nothing and matches the ledgers | Property test and command-line test |
| Containment stays one level, hosts exist and share the site | Property test; ten behaviour tests |
| Each rule does what the model description says | Hand-computed examples per rule (`crates/lab-rules/tests/`) |
| Live viewing and the studio do not change a run | Tests comparing hashes with the headless run |
| Same result on Linux, macOS, Windows | CI workflow `replay`: pinned state hash per example world (`configs/*.sha256`) |
| Analysis formulas | Hand-computed unit tests (`python/tests/test_analysis.py`) |
| Randomization tests, bootstrap, Holm adjustment, verdict rules | Hand-computed unit tests (`test_stats.py`, `test_registered.py`) |

**Can the tests fail?** At M1 the kernel was deliberately broken five ways and each property test
caught its fault (`docs/M1-REPORT.md`). The containment tests were checked against deliberately broken
rules; several first drafts passed regardless of the behaviour and were rewritten.

**Faults found by tests during the build** (kept as a record that the checks bite):

| Fault | Found by |
|---|---|
| A rewrite of conversion dropped the converted-in/out ledger entries | Conservation check in an editor round-trip test |
| A new gene kind was missing from the name table (would have crashed on output) | Test of containment worlds |
| Random test worlds omitted a new required setting | Property tests |
| The dependence measure was zero by construction in one world; a never-consumed resource piled up | Inspecting final genomes of exploratory runs (DR-024) |
| Command-line report rounded fractions to zero; an experiment spec had been committed empty | Following the README; loading every spec |
| Studio charts could stop short of the last tick; the page stayed blank until a first request returned | Browser check in Firefox |
| Analysis of well-mixed worlds ran the machine out of memory | The exploratory sweep itself |

## Instrument verification

`docs/experiments/CONSTRUCT-CHECK.md`. In short: the capability measures and same-site assortment
passed; **both group detectors failed** (they disagree, depend on the threshold and sit near the
shuffled null) and are not used as outcomes. The flux estimator's controls are partly true by
construction: they show it does not invent exchange, not that it attributes mixed supply correctly.

## Output verification and sensitivity

Only exploratory sweeps so far (`docs/experiments/EXPLORATION.md`). No parameter was fitted to data.

## Corroboration

None. The model has not been compared with any empirical system, and no result here is a claim about one.

## Interface checks

The studio and the report were opened in headless Chromium and driven by a script (start a run, change
the map, inspect a site, copy a world, edit and save it); screenshots were inspected. In headless
Firefox a run was started from the page, the map recoloured, the editor and experiment tabs opened
(no console errors) and a report rendered. The README commands were followed in a fresh clone on this
machine. **Not checked:** Safari, Windows or macOS browsers, screen readers, small screens.

## Not yet done

- No independent review of the code (a separate bug sweep is planned; see `docs/BUG-SWEEP.md`).
- Hostile-input testing of the checkpoint decoder is limited to hand-written cases (no fuzzing).
- Performance measured on one laptop.
