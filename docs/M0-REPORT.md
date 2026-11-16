# M0 report — literature pass and benchmark spike

Date: 2026-11-16. Covers PROPOSAL §9 (M0) and §11. Status tags: **[V]** verified this session (search result or run output), **[P]** partly verified (title/abstract level only), **[N]** not checked.

## 1. Benchmark spike (throwaway; source `spikes/m0-bench/bench.rs`)

Workload: 512×512 lattice, 1,048,576 entities × 8 genes, 100 ticks. Integer state only, Philox4x32-10 keyed by (seed, tick, gene, entity), site-local uptake/convert/secrete/maintenance, exact conservation check. `rustc -O`, no `target-cpu=native`, i5-1335U.

```
KAT zero: [6627e8d5, e169c58d, bc57ac4c, 9b00dbd8] expect [6627e8d5, e169c58d, bc57ac4c, 9b00dbd8]
KAT ones: [408f276d, 41c83b0e, a20bc7c6, 6d5451fd] expect [408f276d, 41c83b0e, a20bc7c6, 6d5451fd]
KAT pi:   [d16cfe09, 94fdcceb, 5001e420, 24126ea1] expect [d16cfe09, 94fdcceb, 5001e420, 24126ea1]
threads  1:  13.18s  7.955e6 entity-ticks/s  hash bb13cf64cc5e1977 same_as_1thread=true conservation_exact=true
threads  2:   7.46s  1.405e7 entity-ticks/s  hash bb13cf64cc5e1977 same_as_1thread=true conservation_exact=true
threads  4:   5.20s  2.017e7 entity-ticks/s  hash bb13cf64cc5e1977 same_as_1thread=true conservation_exact=true
threads  8:   3.00s  3.500e7 entity-ticks/s  hash bb13cf64cc5e1977 same_as_1thread=true conservation_exact=true
threads 12:   2.31s  4.545e7 entity-ticks/s  hash bb13cf64cc5e1977 same_as_1thread=true conservation_exact=true
different seed hash 02c821b9635f3d09 (differs: true)
```

What this shows [V]:
- The DR-002/003/004 design works in principle: identical state hash at 1, 2, 4, 8, 12 threads; conservation exact; a different seed gives a different hash.
- Throughput: about 8×10⁶ entity-ticks/s on one thread, 4.5×10⁷ on 12 (a 5.7× speed-up; 8 of the 10 cores are efficiency cores).

What it does not show:
- **Real-kernel speed.** The spike has no movement, reproduction, death, variable-length genomes, cross-site exchange, logging or checkpointing. I expect the real kernel to be several times slower; that is a guess until M1 measures it.
- **Thermal throttling.** Runs lasted 2–13 s; sustained multi-hour load was not tested.
- **Cross-platform equality.** One machine only. CI (DR-013) tests this in M1.
- **The Philox expected vectors** were recalled from memory, not copied from the Random123 distribution. Three independent matches make an error unlikely; M1 takes the vectors from the official `kat_vectors` file.

Planning consequence: at spike speed, 10⁴ entities × 10⁵ ticks is about 22 s per run on 12 threads, so a 1,000-run sweep is about 6 hours. With a 5× slowdown it is about 30 hours. Experiments of that size are feasible on this laptop; 10⁶-entity long runs are not routine. DR-001 (Rust kernel) stands.

## 2. Literature pass

| Item | Finding | Status |
|---|---|---|
| Black Queen hypothesis | Morris, Lenski & Zinser, *mBio* 3(2):e00036-12 (2012). An opinion/hypothesis piece: loss of a costly, leaky function can be selected for, leaving the loser dependent on producers. | [V] |
| Information theory of individuality | Krakauer, Bertschinger, Olbrich, Flack & Ay, *Theory in Biosciences* 139:209–223 (2020). Individuals as entities propagating information from past to future; three forms (organismal, colonial, driven). | [V] |
| Partial information decomposition | Williams & Beer (2010) proposed it; the redundancy measure was criticised (Harder et al. 2013); alternatives by Griffith & Koch, Bertschinger et al.; Rauh et al. showed non-negativity, identity and the original axioms cannot all hold. Contested, as PROPOSAL §5 assumed. | [V] at abstract level |
| Counter-based RNG | Salmon, Moraes, Dror & Shaw, "Parallel random numbers: as easy as 1, 2, 3", SC11 (2011). Philox passes BigCrush per the abstract. | [V] citation; algorithm checked by known-answer test above |
| Division of labour in digital evolution | Goldsby, Dornhaus, Kerr & Ofria, PNAS 2012 (Avida): higher task-switching costs → more division of labour; at high cost individuals could no longer perform tasks alone. | [V] at abstract level |
| Evolved cross-feeding in silico | McNally & Borenstein, *BMC Systems Biology* 2018: dependencies emerged frequently through gene loss with no explicit selection for cooperation. A 2020 *Communications Biology* paper: identical populations reach either a cross-feeding community or an alternative attractor. A 2017 *Nature Communications* paper: paths to obligate cross-feeding via amino-acid leakage. | [P] titles/abstracts; authors of the last two not confirmed |
| Price equation / multilevel selection (Okasha), major transitions (Maynard Smith & Szathmáry; Michod), Fontana & Buss, Tierra, Lenia, Polyworld | Not searched this session. | [N] |

## 3. Consequences for the science (the important part)

1. **RQ1-evolved is not novel as a phenomenon.** Cross-feeding that evolves through gene loss, and dependence that makes individuals non-viable alone, have both been shown in silico (McNally & Borenstein 2018; Goldsby et al. 2012). If the lab merely reproduces "dependence evolves", that is a **validation result**, not a finding. No document may claim it as new (rule 9).
2. **Where a contribution could still lie** (speculative; each needs its own prior-art search before it is claimed):
   - the *conditions for persistence and collapse* of coalitions under controlled perturbation, with null worlds;
   - separating **dependence from synergy** (DR-009), which the cited work does not appear to do in these terms;
   - the same mechanism studied across swappable worlds in one deterministic instrument.
3. **Useful replication targets for validation in M2–M4:** the Black Queen prediction (loss continues until remaining producers just suffice) and the Goldsby result (switching cost → division of labour). Reproducing a known result is a stronger first test of the instrument than a new claim.
4. **The 2020 two-attractor result is a warning:** identical conditions can end in qualitatively different outcomes, so distributions over many seeds are required and a single run means nothing (rule 8).

## 4. Still open
- [N] items in the table above; full-text reading of the [P] items before they are cited in any result.
- Real-kernel throughput and sustained-load behaviour (M1).
- GitHub remote for CI (owner action).
