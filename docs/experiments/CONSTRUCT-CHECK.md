# Construct check of the measures (DR-007, DR-021), 2026-11-26

Runs: `configs/{positive-control,negative-control,blackqueen}.toml`, 5,000 ticks, seed 1, last 8 windows of 20 ticks.
One seed per world: this checks the instruments, it is not a result about the worlds.

## Observed
| Measure | positive | negative | blackqueen | Verdict |
|---|---|---|---|---|
| dependent_fraction / complementary (exact, from genes) | 1.00 / 1 | 0.00 / 0 | 0.22 / 0 | **Pass**: moves with the imposed structure |
| cross_supply_share (flux estimate) | 0.85 | 0.00 | 0.58 | Pass, limited (below) |
| reciprocal_excess, same site (exact) | -0.03 to -0.04 | none (no dependants) | -0.04 to -0.07 | **Pass**: unsaturated; unit tests show it can be positive or negative |
| reciprocal_excess, radius 1 | 0.00 | none | -0.17 | Fail at radius 1: saturates in dense worlds. Same-site radius adopted |
| mutual_fraction, observed vs shuffled (share 0.05) | 0.77 vs 0.69 | 0 vs 0 | 0.21 vs 0.17 | **Fail** |
| mutual_fraction (share 0.20) | 0.00 | 0.00 | 0.01 | **Fail**: result depends on the threshold |
| agreement of the two detectors (share 0.05) | 0.003 | n/a | 0.06 | **Fail** |
| partner_retention vs neighbour_retention (0.05) | 0.50 vs 0.47 | none | 0.19 vs 0.03 | Descriptive only: threshold-dependent |

## Rulings
1. **Detected "coalitions" (D1, D2) are not used as outcomes.** The two detectors disagree almost completely, the
   result swings with the supply threshold, and it sits close to the shuffle null. In dense worlds each taker draws on
   about twenty donors, so no threshold separates partners from neighbours.
2. **Why this is expected:** the world has no partner recognition. Who supplies whom is fully determined by who sits
   where and what each releases and takes, so a flux graph cannot contain more than spatial association of
   complementary types. That association is therefore measured directly and exactly (`assortment.py`).
3. Group-level measures (encapsulation, specialization, Price between/within) are reported as descriptions of the
   D2 partition and carry the same warning.

## Limits of the passes
- The estimator's controls are partly true by construction: with no release there is nothing to attribute, and in the
  positive control only one type releases each product. They show that it does not invent exchange, not that it
  attributes correctly when several donors are mixed.
- The positive control has imposed *dependence* but no mechanism that brings partners together, so it is not a
  positive control for *association*; that rests on the hand-built layouts in `python/tests/test_analysis.py`.

## Repeated after DR-024 (2026-11-26)

The first table above used a dependence measure and worlds that were later corrected (DR-024): the
measure was zero by construction in the symmetric world, and end products piled up. Same procedure on
the corrected worlds and measure (seed 1, 5,000 ticks, last 8 windows of 20 ticks, 20 forked pairs):

| Measure | positive | negative | blackqueen | symmetric |
|---|---|---|---|---|
| dependent_fraction | 1.00 | 0.00 | 0.39 | 0.85 |
| dependent_fraction_leaked | 1.00 | 0.00 | 0.29 | 0.85 |
| complementary | 1 | 0 | 0 | 1 |
| cross_supply_share (estimate) | 0.84 | 0.00 | 0.83 | 0.87 |
| reciprocal_excess, same site | -0.04 | none | 0.00 | -0.18 |
| mutual_fraction, observed vs shuffled | 0.78 vs 0.70 | 0 vs 0 | 0.35 vs 0.27 | 0.07 vs 0.07 |
| forked pairs: dependence / synergy | 23 / 47 | none (no pairs) | 0.25 / 0.5 (4 pairs) | 7.2 / 14 |

Verdicts are unchanged: capability measures pass; group detectors still fail.
