# Model description (ODD)

Structure follows the ODD protocol, 2020 update (Grimm et al. 2020, JASSS 23(2):7). It describes the
code as of 2026-11-26. Parameter values belong to world files (`configs/`), not to the model; where a
number is given here it is an example. If this text and the code disagree, the code is what ran, and
the disagreement is a bug to report.

## 1. Purpose and patterns

**Purpose.** To test whether specified low-level mechanisms (leaky production, costly genes, local
interaction, containment) generate higher-order organization among evolving entities, by comparing a
world with null worlds that differ in one named mechanism.

**Patterns used to judge the model.** No empirical pattern is fitted. Adequacy is judged by instrument
checks (exact conservation, replay, positive and negative controls) and by whether each outcome
measure can come out either way (`docs/TESTING.md`). The model therefore supports "this mechanism can
generate that pattern here", not "this explains an observed system".

## 2. Entities, state variables and scales

| Object | State |
|---|---|
| World | width × height lattice of sites, wrapped or bounded; 1–16 named resources; tick |
| Site | a non-negative integer stock of each resource |
| Entity | id (never reused); site; integer stock of each resource; genome; birth tick; mother id; optional host id |
| Gene | kind (`uptake`, `convert`, `secrete`, `bind`); resource `a` (and output `b` for convert); level (0–65535); sensed resource or "always"; threshold |
| Ledger | per resource: cumulative influx, dissipation, amount converted in and out |

There are no group objects, no fitness variable and no goals. A host–guest tie is the only relation
between entities held in state; it is one level deep.

**Scales.** Space and time are dimensionless. One tick is one pass through the rule list. Example
worlds use 48 × 48 sites, about 10⁴ entities and 10³–10⁴ ticks.

## 3. Process overview and scheduling

Each tick, the world's rules run in the order listed in its file. Each rule reads the state left by
the previous rule, **proposes** changes site by site, and the kernel **applies** them. Rules cannot
change state directly.

Application order within one rule step (`crates/lab-core/src/apply.rs`, DR-015):

1. Each site's proposals apply in proposal order. A withdrawal is clipped to what the holder then has.
2. Credit to a holder at the same site is immediate; credit to another site arrives after all sites
   are processed.
3. Conversions are immediate; the leaked share of a product goes to the entity's site, its host or the sink.
4. Moves, then attachments and detachments. Guests are carried with their host.
5. Deaths and births, in proposal order. An entity removed earlier in the step cannot divide.

After every tick the kernel checks, per resource, exactly:
`total in sites and entities + dissipated + converted_out = initial + influx + converted_in`.
A violation stops the run.

The order in which entities at a site act is drawn afresh every tick where it matters (metabolism,
uptake, binding). Division considers entities in storage order (by creation), so when a site has room
for fewer offspring than there are candidates, **older entities take precedence**. This is a modelling
assumption, not a neutral implementation detail.

## 4. Design concepts

| Concept | In this model |
|---|---|
| Basic principles | Conservation of matter; every gene has a maintenance cost; selection only through differential survival and division; Black Queen mechanism = unavoidably leaky conversion |
| Emergence | Defined per measure before a run: a macro-quantity counts only relative to its distribution in null worlds. Nothing is called emergent because it surprised |
| Adaptation | Evolutionary only (DR-012). Genes sense one resource level at their location and are expressed or not; there is no learning |
| Objectives, learning, prediction | None |
| Sensing | A gene may require at least `threshold` of one resource in the entity's surroundings (site, or host for a guest) |
| Interaction | Indirect through shared site stocks (secretion, leaks, remains of the dead, competition for uptake); direct only between host and guest (sharing) |
| Stochasticity | Order of action, leak rounding, mutation, movement, random birth and death, binding, release, partition, initial placement. All from a counter-based generator keyed by (seed, tick, subject, rule stream, index), so one rule's draws never shift another's, and results do not depend on threads or machine |
| Collectives | Not represented. Hosts with guests are the only physical collective. Any other grouping exists only in analysis |
| Observation | State hashes, per-tick-interval totals (`series.csv`), entity tables, and optionally who took and released what per window. Observation never changes the trajectory |

## 5. Initialization

Every site starts with `init.site_stock`; rectangles in `init.patches` add to it. Each founder type
places `count` identical entities, with given stocks and genome, on uniformly random sites (generator
stream 0). Ids follow the order of founder types. There are no defaults: a missing setting is an error.

## 6. Input data

None. The only external inputs are the world file and the seed. Scheduled shocks are rules in the file.

## 7. Submodels

Amounts are integers; divisions round down. "ppm" means parts per million, compared as integers.

| Rule (version) | What it does |
|---|---|
| `influx.constant` (1) | On ticks divisible by `every`, adds `amount` of a resource to each site from outside |
| `diffusion.integer` (1) | Each site sends `stock / divisor` to each of its up to four neighbours (`divisor ≥ 5`) |
| `decay.fraction` (1) | Each site loses `stock / divisor` |
| `metabolism.genes` (3) | Entities at a site act in random order, genes in genome order. **uptake:** take up to `level`. **convert:** consume up to `level`, produce `consumed × yield_num / yield_den`, only for listed `reactions` (an unlisted pair does nothing); a share `leak_ppm` of the product escapes (remainder rounded at random). **secrete:** release up to `level`. **bind:** no effect here. Then the entity dissipates `base_cost + gene_cost × genes` of the energy resource. `release_to` sends released matter to the surroundings or out of the system; `guests_trade_with` sets whether a guest's surroundings are its host or the site |
| `uptake.constant` (1), `maintenance.constant` (1) | Gene-free uptake and cost, for minimal worlds |
| `move.random` (1) | With a probability, a free entity steps to a random neighbouring site |
| `mix.relocate` (1) | With a probability, a free entity jumps to a random site (well-mixed null) |
| `death.starvation` (1) | An entity holding none of a resource is removed |
| `death.random` (1) | Each entity is removed with a probability |
| `reproduction.division` (3) | An entity holding at least every `requires` threshold divides if its site (or, for a guest, its host) holds fewer than `site_capacity`. It first spends `consume` of each required resource (leaves the system). Offspring takes half of each stock (rounded down) and a mutated genome. A guest's offspring is born in the same host |
| `reproduction.random` (1) | Division with a fixed probability regardless of stocks (neutral null) |
| `shock.scheduled` (1), `shock.cull` (1) | One-off addition or removal of resource in a rectangle; one-off random removal of entities |
| `containment.bind` (1) | Free entities expressing a `bind` gene with the same tag may become guest and host (one level; at most `max_guests`; hosts with guests preferred) |
| `containment.share` (1) | `difference / divisor` of listed resources flows from the richer to the poorer of each host and guest |
| `containment.release` (1) | A guest leaves with a probability |
| `containment.partition` (1) | After a host divides, each guest moves with a probability to the daughter (`lineage`) or to a random eligible host on the site (`random`, the null) |

**Mutation** (applied to each offspring genome, per gene, independent rates in ppm): `delete`;
`point` (level or threshold changes by a uniform step of at most `point_magnitude`); `retarget`
(one identity changes: resource, output, sensed resource, or kind among `kinds`); `duplicate`
(suppressed at `max_genes`). Exact order and draws: `crates/lab-rules/src/mutation.rs`.

**Removal.** A removed entity's stocks go to its living host, otherwise to its site if the removing
rule says `recycle = true`, otherwise out of the system. Guests of a removed host become free.

## Analysis (not part of the model)

Measures are computed from observation files after the fact (`python/evolab/`): functional capability
from genes; a **flux estimate** of who supplied whom (uptake at a site split among nearby releasers in
proportion to what they released, which cannot identify molecules); same-site partner assortment
against a permutation null; counterfactual forks; Price partitions. Which of these passed their
instrument checks is recorded in `docs/experiments/CONSTRUCT-CHECK.md`.

## Known limitations

- Resources are abstract; "metabolism" is a conversion table, with no thermodynamics beyond a yield below one.
- No partner recognition outside the bind tag, no signalling behaviour beyond threshold sensing.
- Space is a coarse lattice; entities on one site are indistinguishable in position.
- One level of containment; hosts cannot be guests.
- Precedence of older entities in division (section 3).
- The dead return what they held to their site; in the example worlds it then decays like other site stocks.
