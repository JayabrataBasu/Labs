"""The Price covariance term split into between-group and within-group parts.

Fitness w_i over a window = 1 if alive at the end, plus the number of its own offspring born in the
window. Selection term on a trait z: Cov(w, z) / mean(w). With individuals partitioned into groups
(ungrouped individuals are groups of one), Cov = Cov_between(group means, size weighted) +
mean_within(Cov inside groups, size weighted). The identity is exact. The transmission term (trait
change from parent to offspring) is not included here, and attributing causal "levels of
selection" to this partition is disputed in the literature; it describes where covariance lies.
"""


def partition(fitness, trait, group_sets):
    ids = sorted(fitness)
    n = len(ids)
    if n == 0:
        return None
    mean_w = sum(fitness[i] for i in ids) / n
    mean_z = sum(trait[i] for i in ids) / n
    if mean_w == 0:
        return None
    assigned = set()
    parts = []
    for g in group_sets:
        members = [i for i in g if i in fitness and i not in assigned]
        if members:
            parts.append(members)
            assigned.update(members)
    parts.extend([i] for i in ids if i not in assigned)
    between = within = 0.0
    for members in parts:
        m = len(members)
        gw = sum(fitness[i] for i in members) / m
        gz = sum(trait[i] for i in members) / m
        between += m / n * (gw - mean_w) * (gz - mean_z)
        within += m / n * sum((fitness[i] - gw) * (trait[i] - gz) for i in members) / m
    selection = sum((fitness[i] - mean_w) * (trait[i] - mean_z) for i in ids) / n
    return {"selection": selection / mean_w, "between": between / mean_w, "within": within / mean_w}


def between_share(fitness, units):
    """Share of the fitness variance among members of multi-member units that lies between units.

    Under random assignment of the same unit sizes its expectation is exactly (g - 1) / (n - 1) for g
    units and n members, so `excess` over that is zero when units do not matter. None if there are
    fewer than two units or no variance.
    """
    kept = [[fitness[i] for i in sorted(u) if i in fitness] for u in units]
    kept = [u for u in kept if len(u) >= 2]
    values = [w for u in kept for w in u]
    if len(kept) < 2:
        return None
    mean = sum(values) / len(values)
    total = sum((w - mean) ** 2 for w in values)
    if total == 0:
        return None
    between = sum(len(u) * (sum(u) / len(u) - mean) ** 2 for u in kept)
    share = between / total
    return {"share": share, "excess": share - (len(kept) - 1) / (len(values) - 1)}


def window_fitness(window):
    """Fitness of every entity alive at the start of an observation window."""
    alive = set(window.entities_end)
    fitness = {i: float(i in alive) for i in window.entities_start}
    for _, _, mother in window.births:
        if mother in fitness:
            fitness[mother] += 1
    return fitness
