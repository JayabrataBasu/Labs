"""Are complementary partners found together more often than chance? Exact, no flux estimator.

A dependant is an entity that depends on a leaked product (what only the dead give back has no
living partner). A neighbour j (within `radius`) **supplies** dependant i if j builds that product. The
relation is **reciprocal** if i also supplies j. The null (N4) keeps every position and every genome
but deals the genomes to positions at random, so it has the same abundances and the same crowding.
`excess` = observed fraction minus the mean over permutations: positive means complementary
partners are spatially associated, negative means they are kept apart (e.g. by like offspring
settling next to their parents).

The world has no partner recognition: association in space is the only form a partnership can take
here, so this is what "coalition" can mean in these experiments.
"""
import random
import statistics

from . import capability
from .flux import neighbourhood


def _fractions(kinds, at, reach):
    """(supplied, reciprocal) fractions of dependants, for kinds[i] = (depends_on, makes)."""
    present = {}
    for site, members in at.items():
        present[site] = {kinds[i] for i in members}
    nearby = {}
    dependants = supplied = reciprocal = 0
    for site, members in at.items():
        if site not in nearby:
            nearby[site] = set().union(*(present.get(u, set()) for u in reach(site)))
        for i in members:
            needs, offers = kinds[i]
            if not needs:
                continue
            dependants += 1
            suppliers = [k for k in nearby[site] if k[1] & needs]
            supplied += bool(suppliers)
            reciprocal += any(k[0] & offers for k in suppliers)
    return (supplied / dependants, reciprocal / dependants) if dependants else None


def measure(entities, chemistry, width, height, wrap, radius, permutations, seed):
    cache, kinds, at = {}, [], {}
    for n, e in enumerate(entities):
        if e.genome not in cache:
            cache[e.genome] = (capability.leaked_dependencies(e.genome, chemistry), capability.makes(e.genome, chemistry))
        kinds.append(cache[e.genome])
        at.setdefault(e.site, []).append(n)
    reached = {}

    def reach(site):
        if site not in reached:
            reached[site] = neighbourhood(site, width, height, wrap, radius)
        return reached[site]

    observed = _fractions(kinds, at, reach)
    if observed is None:
        return {k: None for k in ("supplied_fraction", "reciprocal_fraction", "supplied_excess", "reciprocal_excess")}
    rng = random.Random(seed)
    null = []
    shuffled = kinds[:]
    for _ in range(permutations):
        rng.shuffle(shuffled)
        null.append(_fractions(shuffled, at, reach))
    return {
        "supplied_fraction": observed[0],
        "reciprocal_fraction": observed[1],
        "supplied_excess": observed[0] - statistics.fmean(x[0] for x in null),
        "reciprocal_excess": observed[1] - statistics.fmean(x[1] for x in null),
    }
