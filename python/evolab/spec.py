"""Experiment specifications: expansion into runs, and freezing (DR-008).

A spec is a TOML file. Freezing records its hash together with the hash of the base config and
of every expanded run config. `run` refuses a spec whose frozen record does not match, so the
design and the analysis plan cannot change after outcomes have been seen without leaving a trace.

    name = "rq1-baseline"
    base_config = "configs/crossfeed.toml"      # relative to the spec file
    ticks = 3000
    series_every = 100
    threads = 1                                  # threads per run; runs themselves are parallel
    seeds = [1001, 1002, 1003, 1004, 1005]       # registered runs need seeds >= 1000 (exploration uses < 1000)
    primary_outcome = "entities"                 # column of series.csv, read at the last tick
    aim = "the question this experiment asks"                      # ADEMP: Aims
    estimand = "the quantity compared, e.g. median final population per arm"   # ADEMP: Estimand
    analysis_plan = "methods and performance measures: what is compared and how"   # ADEMP: Methods, Performance

    [observe]                                    # optional: observe the last `count` windows and analyse them
    window = 20
    count = 10
    flux_radius = 1                              # reach of the who-fed-whom estimator
    contact_radius = 0                           # reach for partner association (0 = same site)
    min_share = 0.05                             # a material supplier gives at least this share of a taker's intake
    min_fraction = 0.05                          # a dependency class "exists" above this population share
    permutations = 40                            # shuffles for the chance expectation
    shuffle_seed = 1
    counterfactual_pairs = 0                     # partner pairs to fork (0 = skip)
    counterfactual_ticks = 200
    keep_raw = false                             # keep the raw observation files (large)

    [[arms]]
    name = "intact"
    transforms = []

    [[arms]]
    name = "N1_no_interaction"
    transforms = [{ name = "null.no_secretion" }]

    [[sweeps]]                                   # full factorial over all sweeps
    path = "rules.5.params.divisor"              # rules.<stream>.params.<key>, init.founders.0.count, world.wrap ...
    values = [10, 20, 40]

    [[sweeps]]                                   # several settings moved together need a name
    name = "leak"
    paths = ["rules.8.params.reactions.0.leak_ppm", "rules.8.params.reactions.1.leak_ppm"]
    values = [50000, 200000]
"""
import copy
import hashlib
import itertools
import json
import tomllib
from pathlib import Path

from . import toml_out, transforms

OBSERVE_FIELDS = ["window", "count", "flux_radius", "contact_radius", "min_share", "min_fraction", "permutations",
                  "shuffle_seed", "counterfactual_pairs", "counterfactual_ticks", "keep_raw"]
REQUIRED = ["name", "base_config", "ticks", "series_every", "threads", "seeds", "primary_outcome", "aim", "estimand", "analysis_plan", "arms"]

# Seed firewall (DR-019). Seeds below this are for exploration and tuning; a registered (frozen)
# experiment may only use seeds at or above it, so confirmatory runs are never the runs the design was tuned on.
EXPLORATORY_SEED_LIMIT = 1000


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def load(spec_path):
    spec_path = Path(spec_path)
    text = spec_path.read_text()
    spec = tomllib.loads(text)
    missing = [k for k in REQUIRED if k not in spec]
    if missing:
        raise ValueError(f"spec lacks required fields (there are no defaults): {missing}")
    if not spec["seeds"] or len(set(spec["seeds"])) != len(spec["seeds"]):
        raise ValueError("seeds must be a non-empty list without duplicates")
    names = [a["name"] for a in spec["arms"]]
    if len(set(names)) != len(names):
        raise ValueError("arm names must be unique")
    base_path = (spec_path.parent / spec["base_config"]).resolve()
    if "observe" in spec:
        missing = [k for k in OBSERVE_FIELDS if k not in spec["observe"]]
        extra = [k for k in spec["observe"] if k not in OBSERVE_FIELDS]
        if missing or extra:
            raise ValueError(f"[observe] lacks {missing} and has unknown {extra} (there are no defaults)")
        if spec["observe"]["window"] * spec["observe"]["count"] > spec["ticks"]:
            raise ValueError("[observe] window x count exceeds ticks")
    return spec, text, base_path


def set_path(cfg, path, value):
    """Sets one existing setting, failing if it does not exist. Paths are dotted:
    `rules.<stream>.params.<key>[.<index>...]`, `init.founders.<index>.count`, `world.wrap`, ..."""
    parts = path.split(".")
    node, i = cfg, 0
    if parts[0] == "rules":
        matches = [r for r in cfg["rules"] if len(parts) > 2 and str(r["stream"]) == parts[1]]
        if len(matches) != 1:
            raise ValueError(f"sweep path {path!r}: no rule with that stream")
        node, i = matches[0], 2

    def step(node, part):
        if isinstance(node, list):
            if not part.isdigit() or int(part) >= len(node):
                raise ValueError(f"sweep path {path!r}: no list element {part!r}")
            return int(part)
        if not isinstance(node, dict) or part not in node:
            raise ValueError(f"sweep path {path!r}: {part!r} does not exist (a sweep may only change existing settings)")
        return part

    for part in parts[i:-1]:
        node = node[step(node, part)]
    node[step(node, parts[-1])] = value


def _paths(sweep):
    """A sweep changes one setting (`path`) or several together (`paths`), to the same value."""
    return sweep["paths"] if "paths" in sweep else [sweep["path"]]


def _name(sweep):
    return sweep["name"] if "name" in sweep else sweep["path"].replace(".", "-")


def expand(spec, base_cfg):
    """Every run of the experiment: arm x sweep point x seed, each with its full config."""
    sweeps = spec.get("sweeps", [])
    runs = []
    for arm in spec["arms"]:
        arm_cfg = transforms.apply(base_cfg, arm.get("transforms", []))
        for point in itertools.product(*[s["values"] for s in sweeps]):
            for seed in spec["seeds"]:
                cfg = copy.deepcopy(arm_cfg)
                label = "base"
                for sweep, value in zip(sweeps, point):
                    for path in _paths(sweep):
                        set_path(cfg, path, value)
                if point:
                    label = "_".join(f"{_name(s)}={v}" for s, v in zip(sweeps, point))
                cfg["seed"] = seed
                runs.append({"arm": arm["name"], "point": label, "seed": seed, "config": cfg,
                             "params": {_name(s): v for s, v in zip(sweeps, point)}})
    return runs


def run_hashes(runs):
    return {f"{r['arm']}/{r['point']}/seed-{r['seed']}": sha256(toml_out.dumps(r["config"]).encode()) for r in runs}


def frozen_path(spec_path):
    p = Path(spec_path)
    return p.with_suffix(".frozen.json")


def freeze(spec_path):
    spec, text, base_path = load(spec_path)
    low = [s for s in spec["seeds"] if s < EXPLORATORY_SEED_LIMIT]
    if low:
        raise ValueError(f"a frozen (registered) spec may not use seeds below {EXPLORATORY_SEED_LIMIT}, which are reserved for exploration: {low}")
    base_text = base_path.read_text()
    runs = expand(spec, tomllib.loads(base_text))
    record = {"spec_sha256": sha256(text.encode()), "base_config_sha256": sha256(base_text.encode()),
              "n_runs": len(runs), "run_config_sha256": run_hashes(runs)}
    frozen_path(spec_path).write_text(json.dumps(record, indent=1) + "\n")
    return record


def verify_frozen(spec_path):
    """Returns the list of reasons the frozen record does not match the current spec (empty = matches)."""
    fp = frozen_path(spec_path)
    if not fp.exists():
        return [f"{fp.name} does not exist: freeze the spec first"]
    record = json.loads(fp.read_text())
    spec, text, base_path = load(spec_path)
    base_text = base_path.read_text()
    problems = []
    if sha256(text.encode()) != record["spec_sha256"]:
        problems.append("the spec has changed since it was frozen")
    if sha256(base_text.encode()) != record["base_config_sha256"]:
        problems.append("the base config has changed since the spec was frozen")
    if not problems and run_hashes(expand(spec, tomllib.loads(base_text))) != record["run_config_sha256"]:
        problems.append("the expanded run configs differ from those frozen (transforms or code changed)")
    return problems
