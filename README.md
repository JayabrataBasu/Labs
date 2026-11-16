# Programmable Artificial Evolutionary Laboratory

A deterministic, conserved simulation kernel for studying how higher-order organization arises from
lower-level interactions. Read `CLAUDE.md` for the mission and rules, `docs/PROPOSAL.md` for scope,
`docs/DECISIONS.md` for decisions, and `docs/M1-REPORT.md` for the current state and evidence.

## Use (M1)

```
cargo build --release
./target/release/lab run    configs/reference.toml --ticks 2000 --threads 4 --out runs/ref
./target/release/lab resume runs/ref/ckpt-2000.bin --config configs/reference.toml --ticks 500 --threads 4 --out runs/ref2
./target/release/lab hash   runs/ref/ckpt-2000.bin
./target/release/lab view   configs/reference.toml --ticks 5000 --threads 4 --port 8787 --out runs/view --linger-seconds 600
```

`view` serves a read-only page at `http://127.0.0.1:8787/`. Every run writes `manifest.json` (code version,
config, seed, rule versions, hashes) and checkpoints. The same config gives the same hash at any thread count.

Tests: `cargo test --workspace`. Lint: `cargo clippy --workspace --all-targets -- -D warnings`.
