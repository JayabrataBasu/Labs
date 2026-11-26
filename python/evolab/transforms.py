"""Null worlds and other declared changes to a base config (DR-007).

Each transform takes a config dict and returns a new one. A transform changes exactly one
named mechanism, so an arm's difference from the intact world is its definition.

N1a null.no_secretion - secreted and leaked matter goes out of the system instead of to the site: the
                        producer's budget is unchanged, but nothing is transferred.
N1b null.no_recycling - a dead entity's stocks leave the system instead of returning to its site, which
                        removes the second inter-entity channel (necromass).
N1  = N1a + N1b, the complete "no transfer between entities" null. Run N1a and N1b separately as well:
                        the contrast shows which channel carries an effect.
N2 null.well_mixed    - entities are scattered over the whole lattice every tick: spatial structure removed.
                        This destroys *entity* neighbourhoods; resources still diffuse spatially.
N3 null.neutral       - reproduction and death no longer depend on what genes achieve. Rates must be
                        given explicitly (matched to the intact arm's demography by the experimenter).
N5 null.no_mutation   - every mutation rate is zero: no heritable variation.
N4 (spatially shuffled detector input) is an analysis-level control and arrives with the detector (M4).
"""
import copy


def _rules(cfg, id_):
    return [r for r in cfg["rules"] if r["id"] == id_]


def _new_stream(cfg):
    return max([r["stream"] for r in cfg["rules"]] + [0]) + 1


def _need(cfg, id_, why):
    found = _rules(cfg, id_)
    if not found:
        raise ValueError(f"transform needs a {id_} rule ({why}) but the base config has none")
    return found


def no_secretion(cfg, **_):
    cfg = copy.deepcopy(cfg)
    for rule in _need(cfg, "metabolism.genes", "to silence secretion"):
        rule["params"]["release_to"] = "sink"
    return cfg


def no_recycling(cfg, **_):
    cfg = copy.deepcopy(cfg)
    found = _rules(cfg, "death.starvation") + _rules(cfg, "death.random") + _rules(cfg, "shock.cull")
    if not found:
        raise ValueError("null.no_recycling needs at least one death.* or shock.cull rule")
    for rule in found:
        rule["params"]["recycle"] = False
    return cfg


def well_mixed(cfg, **_):
    cfg = copy.deepcopy(cfg)
    for rule in _rules(cfg, "move.random"):
        rule["params"]["probability_ppm"] = 0
    cfg["rules"].append({"id": "mix.relocate", "version": 1, "stream": _new_stream(cfg), "params": {"probability_ppm": 1_000_000}})
    return cfg


def neutral(cfg, birth_ppm=None, death_ppm=None, **_):
    if birth_ppm is None or death_ppm is None:
        raise ValueError("null.neutral needs explicit birth_ppm and death_ppm: there are no silent defaults")
    cfg = copy.deepcopy(cfg)
    division = _need(cfg, "reproduction.division", "to replace it with random birth")
    template = division[0]["params"]
    recycle = all(r["params"]["recycle"] for r in _rules(cfg, "death.starvation")) if _rules(cfg, "death.starvation") else True
    kept = [r for r in cfg["rules"] if r["id"] not in ("reproduction.division", "death.starvation")]
    births = {"id": "reproduction.random", "version": 1, "stream": division[0]["stream"],
              "params": {"probability_ppm": birth_ppm, "site_capacity": template["site_capacity"], "mutation": template["mutation"]}}
    deaths = {"id": "death.random", "version": 1, "stream": _new_stream(cfg), "params": {"probability_ppm": death_ppm, "recycle": recycle}}
    cfg["rules"] = kept + [births, deaths]
    return cfg


def no_mutation(cfg, **_):
    cfg = copy.deepcopy(cfg)
    found = _rules(cfg, "reproduction.division") + _rules(cfg, "reproduction.random")
    if not found:
        raise ValueError("null.no_mutation needs a reproduction rule")
    for rule in found:
        m = rule["params"]["mutation"]
        for key in ("point_ppm", "retarget_ppm", "duplicate_ppm", "delete_ppm"):
            m[key] = 0
    return cfg


REGISTRY = {
    "null.no_secretion": no_secretion,
    "null.no_recycling": no_recycling,
    "null.well_mixed": well_mixed,
    "null.neutral": neutral,
    "null.no_mutation": no_mutation,
}


def apply(cfg, transforms):
    """Applies a list of `{ name = ..., **options }` in order."""
    for t in transforms:
        options = {k: v for k, v in t.items() if k != "name"}
        if t["name"] not in REGISTRY:
            raise ValueError(f"unknown transform {t['name']!r}; known: {sorted(REGISTRY)}")
        cfg = REGISTRY[t["name"]](cfg, **options)
    return cfg
