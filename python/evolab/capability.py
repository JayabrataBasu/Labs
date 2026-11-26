"""What each genome can do. Exact: read from genes and the world's rules, no detector involved.

A genome **builds** a resource if it can make it from outside supply: it takes up a *primary*
resource (one that enters from outside, or that no reaction makes) and carries the conversions
leading to it. A genome **depends** on a product p if it needs p (upkeep, survival, division, or as
the substrate of a conversion it carries), takes p up, and cannot build p: whatever it gets was made
by others, whether they leaked it, secreted it or left it behind when they died.

A *leaky reaction* is one whose product escapes from the living (leak_ppm > 0). Dependence on a
leaked product is the Black Queen relation, between living neighbours. Dependence on any other
product can be met only from the dead or from secretion.

A gene with level 0 does nothing. Sensing gates are ignored (a gated gene counts as present).
"""
from typing import NamedTuple


class Chemistry(NamedTuple):
    reactions: tuple  # (from, to, leak_ppm) resource indices
    needs: frozenset  # resources every entity requires whatever its genes: upkeep, starvation, division
    primary: frozenset  # resources supplied from outside, or made by no reaction


def chemistry_of(config):
    """The chemistry and requirements of a world config."""
    index = {name: i for i, name in enumerate(config["world"]["resources"])}
    reactions, needs, supplied = [], set(), set()
    for rule in config["rules"]:
        params = rule.get("params", {})
        if rule["id"] == "metabolism.genes":
            reactions = [(index[r["from"]], index[r["to"]], r["leak_ppm"]) for r in params["reactions"]]
            needs.add(index[params["energy"]])
        elif rule["id"] == "death.starvation":
            needs.add(index[params["resource"]])
        elif rule["id"] == "reproduction.division":
            needs.update(index[r["resource"]] for r in params["requires"])
        elif rule["id"] == "influx.constant":
            supplied.add(index[params["resource"]])
    made = {b for _, b, _ in reactions}
    return Chemistry(tuple(reactions), frozenset(needs), frozenset(supplied | (set(index.values()) - made)))


def leaky_reactions(chemistry):
    return [(a, b) for a, b, leak in chemistry.reactions if leak > 0]


def products(chemistry):
    return sorted({b for _, b, _ in chemistry.reactions})


def _abilities(genome, chemistry):
    """(taken up, allowed conversions carried, resources built from outside supply)."""
    active = [g for g in genome if g.level > 0]
    uptake = {g.a for g in active if g.kind == "uptake"}
    allowed = {(a, b) for a, b, _ in chemistry.reactions}
    converts = {(g.a, g.b) for g in active if g.kind == "convert"} & allowed
    have, built, grew = uptake & chemistry.primary, set(), True
    while grew:
        grew = False
        for a, b in converts:
            if a in have and b not in built:
                built.add(b)
                have.add(b)
                grew = True
    return uptake, converts, built


def makes(genome, chemistry):
    """The leaked products this genome builds, and therefore releases while alive."""
    uptake, converts, built = _abilities(genome, chemistry)
    have = (uptake & chemistry.primary) | built
    return frozenset(b for a, b in leaky_reactions(chemistry) if (a, b) in converts and a in have)


def dependencies(genome, chemistry):
    """Every product this genome needs and takes up but cannot build."""
    uptake, converts, built = _abilities(genome, chemistry)
    used = chemistry.needs | {a for a, _ in converts}
    return frozenset(p for p in products(chemistry) if p in used and p in uptake and p not in built)


def leaked_dependencies(genome, chemistry):
    """The dependencies that living neighbours can meet by leaking."""
    return dependencies(genome, chemistry) & {b for _, b in leaky_reactions(chemistry)}


def summary(genomes, chemistry, min_fraction):
    """Population-level capability and dependence. `builds_p`: share that builds product p;
    `lacking_leaked`: share that fails to build at least one leaked product, whether or not it can
    get it from others. `complementary` is 1 when, for two different leaked products p and q, at
    least `min_fraction` of the population depends on p while building q, and at least
    `min_fraction` depends on q while building p."""
    leaked = sorted({b for _, b in leaky_reactions(chemistry)})
    n = len(genomes)
    cache, counts, sides = {}, dict.fromkeys(products(chemistry), 0), {}
    built_by = dict.fromkeys(counts, 0)
    any_dependent = any_leaked = lacking = 0
    for genome in genomes:
        key = tuple(genome)
        if key not in cache:
            cache[key] = (dependencies(genome, chemistry), makes(genome, chemistry), _abilities(genome, chemistry)[2])
        deps, built, all_built = cache[key]
        lacking += not all_built.issuperset(leaked)
        for p in all_built:
            built_by[p] += 1
        any_dependent += bool(deps)
        any_leaked += not deps.isdisjoint(leaked)
        for p in deps:
            counts[p] += 1
        for p in deps.intersection(leaked):
            for q in built - {p}:
                sides[(p, q)] = sides.get((p, q), 0) + 1
    out = {"dependent_fraction": any_dependent / n if n else 0.0,
           "dependent_fraction_leaked": any_leaked / n if n else 0.0,
           "lacking_leaked": lacking / n if n else 0.0}
    for p in counts:
        out[f"dependent_on_{p}"] = counts[p] / n if n else 0.0
        out[f"builds_{p}"] = built_by[p] / n if n else 0.0
    out["complementary"] = int(n > 0 and any(
        count / n >= min_fraction and sides.get((q, p), 0) / n >= min_fraction for (p, q), count in sides.items()))
    return out
