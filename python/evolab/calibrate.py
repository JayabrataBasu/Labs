"""Matches the neutral null (N3) to the intact world's demography.

N3 replaces selection by chance: births and deaths happen with fixed probabilities. To compare like
with like, the extra death probability is set to the intact run's death rate beyond what its own
random death already causes, and the birth probability is searched until the neutral population
settles at the intact run's size. Calibrate on exploratory runs only (DR-019).
"""
import csv
import json
import subprocess
import tomllib
from pathlib import Path

from . import toml_out, transforms


def demography(rows, from_tick):
    """Per-entity, per-tick birth and death rates (ppm) and the mean population from `from_tick` on."""
    used = [{k: int(r[k]) for k in ("tick", "entities", "births", "deaths")} for r in rows if int(r["tick"]) >= from_tick]
    if len(used) < 2:
        raise ValueError(f"need at least two series rows from tick {from_tick} on")
    entity_ticks = sum((a["entities"] + b["entities"]) / 2 * (b["tick"] - a["tick"]) for a, b in zip(used, used[1:]))
    ticks = used[-1]["tick"] - used[0]["tick"]
    return {"birth_ppm": (used[-1]["births"] - used[0]["births"]) / entity_ticks * 1e6,
            "death_ppm": (used[-1]["deaths"] - used[0]["deaths"]) / entity_ticks * 1e6,
            "entities": entity_ticks / ticks}


def extra_death_ppm(config, measured_ppm):
    """The death probability to add to the world's own random death to reach `measured_ppm`."""
    own = sum(r["params"]["probability_ppm"] for r in config["rules"] if r["id"] == "death.random")
    return max(0, round(measured_ppm - own))


def search(population, target, low, high, tolerance, steps):
    """Bisection for the argument at which the increasing function `population` reaches `target`
    within a relative `tolerance`. Returns (argument, value there)."""
    top = population(high)
    if top < target * (1 - tolerance):
        raise ValueError(f"even birth_ppm = {high} reaches only {top:.0f} entities, short of {target:.0f}")
    for _ in range(steps - 1):
        mid = (low + high) // 2
        value = population(mid)
        if abs(value - target) <= tolerance * target:
            return mid, value
        low, high = (mid, high) if value < target else (low, mid)
    raise ValueError(f"no birth probability within {tolerance:.0%} of {target:.0f} entities after {steps} tries")


def _series(path):
    with open(path, newline="") as f:
        return list(csv.DictReader(f))


def run(binary, run_dir, from_tick, ticks, workdir, tolerance=0.02):
    """Calibrates N3 against the finished run in `run_dir`."""
    run_dir, workdir = Path(run_dir), Path(workdir)
    config = tomllib.loads(json.loads((run_dir / "manifest.json").read_text())["config_text"])
    measured = demography(_series(run_dir / "series.csv"), from_tick)
    death = extra_death_ppm(config, measured["death_ppm"])
    workdir.mkdir(parents=True, exist_ok=True)

    def population(birth_ppm):
        out = workdir / f"birth-{birth_ppm}"
        out.mkdir(parents=True, exist_ok=True)
        neutral = transforms.apply(config, [{"name": "null.neutral", "birth_ppm": birth_ppm, "death_ppm": death}])
        (out / "config.toml").write_text(toml_out.dumps(neutral))
        done = subprocess.run([binary, "run", str(out / "config.toml"), "--ticks", str(ticks), "--threads", "1", "--out", str(out),
                               "--series-every", str(max(1, ticks // 20))], capture_output=True, text=True)
        if done.returncode != 0:
            raise RuntimeError(f"lab run failed: {done.stderr.strip()}")
        return demography(_series(out / "series.csv"), ticks // 2)["entities"]

    birth, reached = search(population, measured["entities"], 0, 1_000_000, tolerance, 24)
    return {"birth_ppm": birth, "death_ppm": death, "entities": reached, "target_entities": measured["entities"],
            "intact_birth_ppm": measured["birth_ppm"], "intact_death_ppm": measured["death_ppm"]}
