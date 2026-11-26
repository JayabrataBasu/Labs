"""Dependence and synergy by forking the world (DR-009).

Pairs are taken from the exact partner relation (same-site, reciprocal supply), not from the flux
estimator. From the run's final checkpoint three worlds are run on: the pair alone together, and
each member alone. "Output" of a member is its lineage's size: itself if alive plus its descendants.

synergy(i, j)    = output(i, j together) - output(i alone) - output(j alone)
dependence(i|j)  = output of i with j - output of i alone

Confounds, always report them with the numbers: (1) all other entities are removed in every fork,
so what the community left on the sites is available to solo and pair alike and runs down over the
horizon; (2) two members compete for the same site, which pushes synergy down (conservative);
(3) descendants' random draws differ between forks.
"""
import json
import random
import statistics
import subprocess
from pathlib import Path

from . import capability


def choose_pairs(entities, chemistry, count, seed):
    """Up to `count` disjoint same-site pairs that depend on each other's products."""
    at = {}
    for e in sorted(entities, key=lambda e: e.id):
        at.setdefault(e.site, []).append(e)
    candidates = []
    for here in at.values():
        info = [(e.id, capability.dependencies(e.genome, chemistry), capability.makes(e.genome, chemistry)) for e in here]
        for x, (i, needs_i, makes_i) in enumerate(info):
            for j, needs_j, makes_j in info[x + 1:]:
                if needs_i & makes_j and needs_j & makes_i:
                    candidates.append((i, j))
    random.Random(seed).shuffle(candidates)
    used, chosen = set(), []
    for i, j in candidates:
        if len(chosen) < count and i not in used and j not in used:
            chosen.append((i, j))
            used.update((i, j))
    return sorted(chosen)


def request(pairs):
    sets = []
    for n, (i, j) in enumerate(pairs):
        sets.append({"name": f"{n}:pair", "keep": [i, j], "report": [i, j]})
        sets.append({"name": f"{n}:first", "keep": [i], "report": [i]})
        sets.append({"name": f"{n}:second", "keep": [j], "report": [j]})
    return {"sets": sets}


def output(member):
    return int(member["alive"]) + member["descendants"]


def score(result):
    """Mean synergy and dependence over the pairs of a `lab counterfactual` result."""
    by_name = {r["name"]: r["members"] for r in result["results"]}
    synergy, dependence = [], []
    for name in sorted(n for n in by_name if n.endswith(":pair")):
        n = name.split(":")[0]
        together = [output(m) for m in by_name[name]]
        alone = [output(by_name[f"{n}:first"][0]), output(by_name[f"{n}:second"][0])]
        synergy.append(sum(together) - sum(alone))
        dependence.extend(t - a for t, a in zip(together, alone))
    return {"pairs_tested": len(synergy),
            "synergy": statistics.fmean(synergy) if synergy else None,
            "dependence": statistics.fmean(dependence) if dependence else None}


def run(binary, checkpoint, config, entities, chemistry, count, ticks, seed, workdir):
    pairs = choose_pairs(entities, chemistry, count, seed)
    if not pairs:
        return score({"results": []})
    workdir = Path(workdir)
    (workdir / "fork-sets.json").write_text(json.dumps(request(pairs)))
    done = subprocess.run([binary, "counterfactual", str(checkpoint), "--config", str(config), "--sets", str(workdir / "fork-sets.json"),
                           "--ticks", str(ticks), "--threads", "1", "--out", str(workdir / "forks.json")], capture_output=True, text=True)
    if done.returncode != 0:
        raise RuntimeError(f"lab counterfactual failed: {done.stderr.strip()}")
    return score(json.loads((workdir / "forks.json").read_text()))
