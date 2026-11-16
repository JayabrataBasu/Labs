# Decision records

Append-only. Never edit a record's body; supersede with a new one and update only the Status line.
A record is written only when a decision changes a scientific interface or a hard-to-reverse architecture choice.
Soft cap: 15 through M2. Rationale and detail live in `PROPOSAL.md` (linked by section), not here.

| # | Decision | Detail | Status |
|---|---|---|---|
| DR-001 | Technology: Rust kernel, Python lab (PyO3), browser UI, TOML config validated by schema | PROPOSAL §7 | Approved 2026-11-16 |
| DR-002 | Kernel state is integer/fixed-point only; floats are analysis-only | PROPOSAL §4.4 | Approved 2026-11-16 |
| DR-003 | Counter-based keyed RNG; key = (seed, stream, tick, entity, purpose) | PROPOSAL §4.4 | Approved 2026-11-16 |
| DR-004 | Strict integer conservation with explicit influx and dissipation, checked exactly every tick | PROPOSAL §4.3 | Approved 2026-11-16 |
| DR-005 | Core has no group primitive; groups are detected post hoc by analysis | PROPOSAL §4.1, §5 | Approved 2026-11-16 |
| DR-006 | Genome = variable-length list of typed genes with maintenance cost; independent mutation operators | PROPOSAL §4.2 | Approved 2026-11-16 |
| DR-007 | Every experiment ships null worlds N1–N5 and passes a construct check before registered runs | PROPOSAL §5 | Approved 2026-11-16 |
| DR-008 | Experiment spec is frozen and hashed, with its analysis plan, before outcomes are seen | PROPOSAL §4.5 | Approved 2026-11-16 |
| DR-009 | Synergy = super-additivity vs solo counterfactuals; ablation loss alone is dependence, not synergy | PROPOSAL §5 | Approved 2026-11-16 |
| DR-010 | Decision-record policy: this single file, soft cap, supersede-only | PROPOSAL §2, §8 | Approved 2026-11-16 |
| DR-011 | Entity model reserves a `parent` containment slot from M1; fusion implemented in M5 | PROPOSAL §1 (Q15), §9 | Approved 2026-11-16 |
| DR-012 | Evolutionary adaptation only in v1; plasticity is a later ablatable module | PROPOSAL §1 (Q16) | Approved 2026-11-16 |
| DR-013 | Cross-platform replay is verified by CI (GitHub Actions: Linux, macOS, Windows) comparing state hashes; audience = theory-building at publication-grade rigor | PROPOSAL §12 | Approved 2026-11-16 |
| DR-014 | Parallel delta application over fixed, thread-independent site blocks with an ordered second pass for cross-block transfers; site index built once per tick unless entities moved. Acceptance: all tests and the pinned reference hash unchanged, measured speed-up | M1-REPORT §6 | Approved 2026-11-16 |
| DR-015 | Delta application semantics within one rule step, defined per site (independent of chunking and threads): a site's deltas apply in proposal order with clipping; same-site credits are immediate; credits to another site arrive only after all sites are processed; moves, deaths and births take effect last. Refines the "ordered second pass" wording of DR-014. Detail: `crates/lab-core/src/apply.rs` header | M2-REPORT | Implemented under DR-014 approval; awaiting owner confirmation |
| DR-016 | Checkpoint format version 2 (genomes, lineage fields, conversion ledgers, birth/death counters) and config schema additions (`init.patches`, `init.genome`). The reference hash is re-pinned: `b31c045e…ecc9f`. Conservation identity becomes `total + dissipated + converted_out == initial + influx + converted_in` | M2-REPORT | Proposed (implemented in M2 branch); awaiting owner confirmation |
| DR-017 | Chemistry is a world rule: `metabolism.genes` takes a `reactions` list; a convert gene whose pair is not listed does nothing (and still costs). Genes mutate freely, the world decides what is possible | M2-REPORT | Proposed; awaiting owner confirmation |
