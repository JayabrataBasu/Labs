"""The hypotheses a spec declares, evaluated from a results folder.

They are part of the spec, so a frozen spec fixes them before any outcome is seen:

    [inference]
    seed = 1                       # for every random procedure
    resamples = 100000             # randomization tests, when too large to enumerate
    bootstrap_resamples = 10000
    alpha = 0.05                   # applied to Holm-adjusted p-values across this spec's hypotheses

    [[hypotheses]]
    name = "H1"
    test = "difference"            # mean(arm) - mean(against); "against_zero": mean(arm)
    metric = "complementary"       # a column of summary.csv
    arm = "intact"
    against = "N2_well_mixed"      # "difference" only
    smallest_effect = 0.2          # the smallest size that counts

Each hypothesis is two-sided and evaluated separately in every sweep point. Verdicts: "supported"
(adjusted p below alpha and the estimate at least the smallest effect), "smaller than the threshold"
(the whole 95% interval lies inside it), otherwise "inconclusive".
"""
import json
import tomllib
from pathlib import Path

from . import analysis, stats

INFERENCE_FIELDS = {"seed", "resamples", "bootstrap_resamples", "alpha"}
COMMON_FIELDS = {"name", "test", "metric", "arm", "smallest_effect"}
TESTS = {"difference": COMMON_FIELDS | {"against"}, "against_zero": COMMON_FIELDS}


def check(spec):
    """Raises ValueError if the declared hypotheses are incomplete or refer to arms that do not exist."""
    hypotheses = spec.get("hypotheses", [])
    if not hypotheses:
        return
    if set(spec.get("inference", {})) != INFERENCE_FIELDS:
        raise ValueError(f"[inference] needs exactly {sorted(INFERENCE_FIELDS)} (there are no defaults)")
    arms = {a["name"] for a in spec["arms"]}
    for h in hypotheses:
        if h.get("test") not in TESTS or set(h) != TESTS[h["test"]]:
            raise ValueError(f"hypothesis {h.get('name')!r}: test must be one of {sorted(TESTS)} with exactly its fields")
        unknown = [h[k] for k in ("arm", "against") if k in h and h[k] not in arms]
        if unknown:
            raise ValueError(f"hypothesis {h['name']!r}: no arm {unknown}")
        if not h["smallest_effect"] > 0:
            raise ValueError(f"hypothesis {h['name']!r}: smallest_effect must be above zero")
    if len({h["name"] for h in hypotheses}) != len(hypotheses):
        raise ValueError("hypothesis names must be unique")


def evaluate(root):
    """One result per hypothesis and sweep point, in the order declared. Empty if none are declared."""
    root = Path(root)
    spec = tomllib.loads(json.loads((root / "experiment.json").read_text()).get("spec_text", ""))
    if not spec.get("hypotheses"):
        return []
    check(spec)
    inference = spec["inference"]
    rows = analysis.load_summary(root)
    points = list(dict.fromkeys(r["point"] for r in rows))
    results = [_one(h, point, rows, inference) for h in spec["hypotheses"] for point in points]
    adjusted = stats.holm({i: r["p"] for i, r in enumerate(results) if r["p"] is not None})
    for i, r in enumerate(results):
        r["p_adjusted"] = adjusted.get(i)
        r["verdict"] = _verdict(r, inference["alpha"])
    return results


def _one(h, point, rows, inference):
    def values(arm):
        cells = [str(r.get(h["metric"], "")) for r in rows if r["arm"] == arm and r["point"] == point]
        return [float(c) for c in cells if c != ""], sum(c == "" for c in cells)

    a, missing = values(h["arm"])
    two = h["test"] == "difference"
    b, missing_against = values(h["against"]) if two else ([], 0)
    out = {"name": h["name"], "point": point, "metric": h["metric"], "arm": h["arm"], "against": h.get("against"),
           "smallest_effect": h["smallest_effect"], "n": len(a), "n_against": len(b) if two else None,
           "missing": missing + missing_against, "estimate": None, "low": None, "high": None, "p": None,
           "varies": len(set(a + b)) > 1}
    if not a or (two and not b):
        return out
    seed = inference["seed"]
    out["estimate"], out["low"], out["high"] = stats.bootstrap_difference(a, b if two else [0.0], seed, inference["bootstrap_resamples"])
    test = (lambda: stats.permutation_test(a, b, seed, inference["resamples"])) if two else \
           (lambda: stats.sign_flip_test(a, seed, inference["resamples"]))
    out["p"] = test()
    return out


def _verdict(r, alpha):
    if r["p"] is None:
        return "not evaluable"
    if not r["varies"]:
        return "inconclusive: the outcome does not vary"
    if r["p_adjusted"] < alpha and abs(r["estimate"]) >= r["smallest_effect"]:
        return "supported"
    if -r["smallest_effect"] < r["low"] and r["high"] < r["smallest_effect"]:
        return "smaller than the threshold"
    return "inconclusive"
