"""What each genome can do. Exact: read from genes and the world's chemistry, no detector involved.

A *leaky reaction* is one whose product escapes (leak_ppm > 0). A genome **depends** on product p
if it uses p (has a conversion from p) and takes p up, but cannot make p itself. "Can make p" means
it has an active conversion into p along a leaky reaction and takes up that reaction's substrate.
A gene with level 0 does nothing. Sensing gates are ignored (a gated gene counts as present).
"""


def leaky_reactions(chemistry):
    """`chemistry` is a list of (from, to, leak_ppm) resource indices."""
    return [(a, b) for a, b, leak in chemistry if leak > 0]


def chemistry_of(config):
    """The (from, to, leak_ppm) list of a config's metabolism rule."""
    index = {name: i for i, name in enumerate(config["world"]["resources"])}
    for rule in config["rules"]:
        if rule["id"] == "metabolism.genes":
            return [(index[r["from"]], index[r["to"]], r["leak_ppm"]) for r in rule["params"]["reactions"]]
    return []


def makes(genome, chemistry):
    """The leaked products this genome can produce: an active leaky conversion and uptake of its substrate."""
    active = [g for g in genome if g.level > 0]
    uptake = {g.a for g in active if g.kind == "uptake"}
    converts = {(g.a, g.b) for g in active if g.kind == "convert"}
    return frozenset(b for a, b in leaky_reactions(chemistry) if (a, b) in converts and a in uptake)


def dependencies(genome, chemistry):
    """The set of leaked products this genome lives on without being able to make them."""
    active = [g for g in genome if g.level > 0]
    uptake = {g.a for g in active if g.kind == "uptake"}
    converts = {(g.a, g.b) for g in active if g.kind == "convert"}
    allowed = {(a, b) for a, b, _ in chemistry}
    out = set()
    for substrate, product in leaky_reactions(chemistry):
        uses = any(a == product and (a, b) in allowed for a, b in converts)
        makes = (substrate, product) in converts and substrate in uptake
        if uses and product in uptake and not makes:
            out.add(product)
    return frozenset(out)


def summary(genomes, chemistry, min_fraction):
    """Population-level dependence. `complementary` is 1 when, for at least two different products,
    at least `min_fraction` of the population depends on it."""
    products = sorted({b for _, b in leaky_reactions(chemistry)})
    n = len(genomes)
    cache, counts, any_dependent = {}, dict.fromkeys(products, 0), 0
    for genome in genomes:
        key = tuple(genome)
        if key not in cache:
            cache[key] = dependencies(genome, chemistry)
        deps = cache[key]
        any_dependent += bool(deps)
        for p in deps:
            counts[p] += 1
    out = {"dependent_fraction": any_dependent / n if n else 0.0}
    for p in products:
        out[f"dependent_on_{p}"] = counts[p] / n if n else 0.0
    out["complementary"] = int(n > 0 and sum(counts[p] / n >= min_fraction for p in products) >= 2)
    return out
