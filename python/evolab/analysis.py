"""Distributions over seeds, and the construct check (DR-007). Standard library only."""
import csv
import statistics
from collections import defaultdict
from pathlib import Path


def load_summary(root):
    with open(Path(root) / "summary.csv", newline="") as f:
        return list(csv.DictReader(f))


def distributions(rows, metric):
    """{(arm, point): [values over seeds]} for one numeric column."""
    groups = defaultdict(list)
    for row in rows:
        groups[(row["arm"], row["point"])].append(float(row[metric]))
    return groups


def describe(values):
    values = sorted(values)
    n = len(values)
    q = statistics.quantiles(values, n=4, method="inclusive") if n > 1 else [values[0]] * 3
    return {"n": n, "mean": statistics.fmean(values), "sd": statistics.stdev(values) if n > 1 else 0.0,
            "min": values[0], "q1": q[0], "median": q[1], "q3": q[2], "max": values[-1]}


def table(rows, metric):
    out = []
    for (arm, point), values in sorted(distributions(rows, metric).items()):
        out.append({"arm": arm, "point": point, **describe(values)})
    return out


def construct_check(rows, metric, intact="intact"):
    """Can this outcome come out false, and does it respond? Returns a list of (ok, message)."""
    groups = distributions(rows, metric)
    findings = []
    everything = [v for vals in groups.values() for v in vals]
    findings.append((len(set(everything)) > 1, f"{metric} varies across runs" if len(set(everything)) > 1 else f"{metric} is constant in every run: it cannot test anything"))
    arms = {a for a, _ in groups}
    findings.append((intact in arms, f"intact arm {intact!r} is present" if intact in arms else f"no arm named {intact!r}: cannot compare against the intact world"))
    nulls = sorted(a for a in arms if a.startswith("N"))
    findings.append((bool(nulls), f"null arms present: {nulls}" if nulls else "no null-world arms (N1-N5): any effect has no baseline"))
    seed_counts = {len(v) for v in groups.values()}
    findings.append((min(seed_counts) >= 5, f"seeds per cell: {sorted(seed_counts)}" + ("" if min(seed_counts) >= 5 else " (fewer than 5: distributions are not meaningful)")))
    same_seed_identical = [k for k, v in groups.items() if len(v) > 1 and len(set(v)) == 1]
    if same_seed_identical:
        findings.append((False, f"{metric} is identical across all seeds in {same_seed_identical[:3]}: check that seeds actually matter (the F01 failure of the earlier project)"))
    return findings
