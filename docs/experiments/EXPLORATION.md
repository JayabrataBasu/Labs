# Exploration log (DR-019)

Three rounds at most, seeds below 1000, no inference. Their only use is to choose a registered design,
which then runs on fresh seeds. Specifications are in `experiments/explore-*.toml`. Older results were
produced by the code at the commit named; those specifications later gained `max_flux_pairs`.

## Round 1: where does dependence evolve? (commit 4065360 to 302bc9a)

Black Queen world, leak {50k, 200k, 500k ppm} x gene cost {0, 1, 2} x walk probability {0, low, high
levels as in the spec}, 3 seeds, 5,000 ticks: 81 runs. Data: `data/explore-1.csv`.

- Complementary coexistence in 2 of 81 runs, one each in two cells, both with the highest leak.
- Mean dependent fraction was highest (0.27 to 0.29) with the highest leak, a gene cost and the most
  movement.
- With a gene cost of 1 or more and movement, up to 0.93 of entities took up an end product they did
  not make.

## Round 2: longer horizon, and a symmetric world (commit bae80f6)

2a: Black Queen world, 20,000 ticks, 3 seeds. 2b: a world with two equivalent leaky pathways,
10,000 ticks, 18 cells. The raw results were lost when the machine rebooted; they can be regenerated
from the commit. Figures below were noted at the time.

| Cell (2a) | Complementary | Dependent fraction | Share taking F without making it |
|---|---|---|---|
| leak 500k, cost 1 | 1 / 3 | 0.22 (0.11 to 0.29) | 0.30 |
| leak 800k, cost 1 | 2 / 3 | 0.12 | 0.58 |
| cost 2 cells | 0 | 0.002 | 0.94 |

2b: dependent fraction **0.000 in every cell**, while 0.37 to 0.99 of entities took up a product they
did not make.

### The fault this exposed (DR-024)

The zero in 2b was a property of the measure and the world, not a finding:

1. In the symmetric world division consumed the two intermediates directly, and "dependence" counted
   only needs met through conversions. It could not be non-zero.
2. End products were never consumed or decayed. They piled up on sites (tens to hundreds of thousands
   of units per resource), so lineages that stopped making them lived on the accumulation, and that
   was not counted as dependence.

DR-024 changed the world (division spends material; end products decay) and the definition
(dependence = needs and takes up a product it cannot build from outside supply; "leaked" dependence
restricts to products that living entities leak). **Rounds 1 and 2 are therefore not comparable with
later results** and support no claim. The construct check was repeated afterwards
(`CONSTRUCT-CHECK.md`).

## Round 3: corrected world with every null arm (commit 968e66b)

Both worlds, five arms, 5 seeds, 10,000 ticks, 50 runs, 17 minutes. Data: `data/explore-3-*.csv`.
Means over seeds (range in brackets where it matters).

| World, measure | intact | N1a no release | N1b no recycling | N2 well mixed | N3 neutral |
|---|---|---|---|---|---|
| blackqueen: entities | 11017 | 10526 | 10671 | 14132 | 10132 |
| blackqueen: complementary | 0 | 0 | 0 | 0 | 0 |
| blackqueen: dependent_fraction_leaked | 0.12 (0.08 to 0.20) | 0.04 | 0.23 | 0.17 | 0.26 |
| blackqueen: lacking_leaked | 0.61 | 0.67 | 0.25 | 0.75 | 0.92 |
| blackqueen: reciprocal_excess | -0.001 | 0 | -0.002 | 0.000 | -0.003 |
| symmetric: entities | 11570 | 11578 | 11346 | 19102 | 11532 |
| symmetric: complementary | 1.00 | 1.00 | 1.00 | 0.60 (0 to 1) | 0 |
| symmetric: dependent_fraction_leaked | 0.96 | 0.96 | 0.24 | 0.86 | 0 |
| symmetric: lacking_leaked | 0.97 | 0.97 | 0.27 | 0.87 | 0.77 |
| symmetric: reciprocal_excess | -0.14 | -0.15 | -0.10 | 0.000 | none |

What this changed in the design:

- **Recycling, not release by the living, carries most dependence in the symmetric world.** Removing
  leaks and secretion changes nothing; removing recycling drops dependence from 0.96 to 0.24. The
  earlier expectation that "dependants cannot live without transfer by construction" (the N1 sanity
  control) does not hold for either removal alone; both together were not run in this round. H4 and
  H5 were added to test this on fresh seeds.
- In the blackqueen world complementary coexistence never occurred, so H1 is expected to be
  uninformative there. It is kept so that both worlds face the same hypotheses.
- Partners are, if anything, found together *less* often than chance (symmetric intact), consistent
  with offspring settling next to like relatives. N2 sits at zero, as it should.
- The neutral null had been calibrated on 5,000-tick runs; in the blackqueen world its turnover was
  higher than the intact world's. It was recalibrated on round-3 intact seed 1 (ticks 5,000 to
  10,000): blackqueen birth 250000, death 1206 ppm; symmetric birth 500000, death 124 ppm.
- Seeds: 30 per arm. For complementary coexistence at the exploratory rates (about 1.0 against 0.6),
  a two-sided exact test at 0.005 has roughly 0.95 power with 30 seeds and 0.6 with 20. The N2 rate
  rests on 5 runs, so this is a rough guide.
- In the well-mixed world the who-fed-whom graph was too large to build and its measures are blank.

Nothing here is a result. The registered design is in `experiments/rq1-*.toml`.
