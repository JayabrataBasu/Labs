"""Measures of one observation window, and of a run's sequence of windows.

All of these describe the simulated world through a stated estimator (see flux.py, groups.py).
None of them establishes anything about real organisations.
"""
import math
import random
import statistics

from . import assortment, capability, data, flux, groups, price


def encapsulation(group, edges):
    """Share of the exchange flux touching the group that stays inside it. Self-supply excluded."""
    return encapsulations([group], edges)[0]


def encapsulations(group_sets, edges):
    """`encapsulation` for many disjoint groups in one pass over the edges."""
    where = {m: k for k, g in enumerate(group_sets) for m in g}
    inside = [0.0] * len(group_sets)
    touching = [0.0] * len(group_sets)
    for (i, j), amount in edges.items():
        if i == j:
            continue
        a, b = where.get(i), where.get(j)
        if a is not None:
            touching[a] += amount
            if a == b:
                inside[a] += amount
        if b is not None and b != a:
            touching[b] += amount
    return [i / t if t else None for i, t in zip(inside, touching)]


def specialization(profiles):
    """Normalised mutual information I(member; channel) / H(channel) of a members x channels flux
    table: 0 when every member has the same flux profile, 1 when each channel has a single member."""
    total = sum(map(sum, profiles))
    if total <= 0:
        return None
    columns = [sum(c) for c in zip(*profiles)]
    entropy = -sum(c / total * math.log(c / total) for c in columns if c > 0)
    if entropy <= 0:
        return 0.0
    information = 0.0
    for row in profiles:
        r = sum(row)
        for value, c in zip(row, columns):
            if value > 0:
                information += value / total * math.log(value * total / (r * c))
    return max(0.0, min(1.0, information / entropy))


def profiles_for(group_sets, flows, resources):
    """For each group, per member: released and taken amount of each exchange resource."""
    index = {r: k for k, r in enumerate(resources)}
    where = {m: k for k, g in enumerate(group_sets) for m in g}
    tables = [{m: [0.0] * (2 * len(resources)) for m in sorted(g)} for g in group_sets]
    for f in flows:
        g, k = where.get(f.entity), index.get(f.resource)
        if g is not None and k is not None:
            row = tables[g][f.entity]
            row[2 * k] += f.released
            row[2 * k + 1] += f.taken
    return [list(t.values()) for t in tables]


def shuffle_profiles(flows, rng):
    """Null world N4: every entity keeps the sites it visited but receives another entity's
    flux amounts, so any association between complementary partners and location is destroyed."""
    by_entity = {}
    for f in flows:
        by_entity.setdefault(f.entity, []).append(f)
    ids = sorted(by_entity)
    donors = ids[:]
    rng.shuffle(donors)
    out = []
    for me, other in zip(ids, donors):
        mine = by_entity[me]
        sites = sorted({f.site for f in mine})
        # The other's records, laid out over my sites in order.
        position = {s: sites[k % len(sites)] for k, s in enumerate(sorted({f.site for f in by_entity[other]}))}
        out.extend(f._replace(entity=me, site=position[f.site]) for f in by_entity[other])
    return out


def exchange_resources(config):
    """Resources entities can pass to each other: products of reactions."""
    return capability.products(capability.chemistry_of(config))


def neighbour_pairs(window, radius, dependencies):
    """Pairs alive at the window's end, within reach of each other, whose dependencies differ:
    potential partners by type and location. The base rate for partner retention."""
    at = {}
    for e in window.entities_end.values():
        at.setdefault(e.site, []).append(e.id)
    pairs = {}
    for site, here in at.items():
        for other in flux.neighbourhood(site, window.width, window.height, window.wrap, radius):
            for i in here:
                for j in at.get(other, ()):
                    if i < j and dependencies[i] != dependencies[j]:
                        pairs[(i, j)] = 1.0
    return pairs


def _mean(values):
    values = [v for v in values if v is not None]
    return statistics.fmean(values) if values else None


def host_units(entities):
    """Hosts together with their guests, as sets of ids (DR-022)."""
    units = {}
    for e in entities.values():
        if e.host is not None:
            units.setdefault(e.host, {e.host}).add(e.id)
    return [units[h] for h in sorted(units)]


def host_composition(entities, chemistry):
    """Who contains whom: share of guests, number of hosts, and the share of guest-host pairs that
    depend on each other's products."""
    guests = [e for e in entities.values() if e.host is not None and e.host in entities]
    reciprocal = 0
    for g in guests:
        h = entities[g.host]
        needs_g, needs_h = (capability.dependencies(x.genome, chemistry) for x in (g, h))
        reciprocal += bool(needs_g & capability.makes(h.genome, chemistry)) and bool(needs_h & capability.makes(g.genome, chemistry))
    return {
        "hosted_fraction": len(guests) / len(entities) if entities else 0.0,
        "hosts": len({g.host for g in guests}),
        "host_reciprocal": reciprocal / len(guests) if guests else None,
    }


def analyse_window(window, chemistry, resources, settings):
    """All per-window measures. `settings` has flux_radius, contact_radius, min_share, min_fraction,
    permutations and shuffle_seed."""
    radius, min_share = settings["flux_radius"], settings["min_share"]
    edges = flux.attribute(window.flows, window.width, window.height, window.wrap, resources, radius)
    intake = flux.intake(window.flows, set(resources))
    pairs = groups.mutual_pairs(edges, intake, min_share)
    d1 = groups.components(pairs)
    d2 = groups.label_propagation(pairs)
    shuffled = shuffle_profiles(window.flows, random.Random(settings["shuffle_seed"] + window.start))
    null_edges = flux.attribute(shuffled, window.width, window.height, window.wrap, resources, radius)
    null_pairs = groups.mutual_pairs(null_edges, flux.intake(shuffled, set(resources)), min_share)

    n = len(window.entities_end)
    active = {f.entity for f in window.flows}
    grouped = set().union(*d1) if d1 else set()
    cross = sum(v for (i, j), v in edges.items() if i != j)
    total_intake = sum(intake.values())
    out = {"population": n}
    out.update(capability.summary([e.genome for e in window.entities_end.values()], chemistry, settings["min_fraction"]))
    out.update({
        "cross_supply_share": cross / total_intake if total_intake else 0.0,
        "mutual_pairs": len(pairs),
        "mutual_fraction": len(grouped) / len(active) if active else 0.0,
        "mutual_fraction_n4": len(set().union(*groups.components(null_pairs)) if null_pairs else ()) / len(active) if active else 0.0,
        "groups_d1": len(d1),
        "largest_group_d1": max(map(len, d1), default=0),
        "groups_d2": len(d2),
        "mean_group_size_d2": _mean([len(g) for g in d2]) or 0.0,
        "detector_agreement": groups.agreement(d1, d2),
        "encapsulation": _mean(encapsulations(d2, edges)),
        "specialization": _mean([specialization(t) for t in profiles_for(d2, window.flows, resources)]),
    })
    out.update(assortment.measure(list(window.entities_end.values()), chemistry, window.width, window.height, window.wrap,
                                  settings["contact_radius"], settings["permutations"], settings["shuffle_seed"] + window.start))
    # Price: trait = number of leaked products an individual depends on.
    deps = {i: capability.leaked_dependencies(e.genome, chemistry) for i, e in window.entities_start.items()}
    parts = price.partition(price.window_fitness(window), {i: float(len(d)) for i, d in deps.items()}, d2)
    for key in ("selection", "between", "within"):
        out[f"price_{key}"] = parts[key] if parts else None
    out.update(host_composition(window.entities_end, chemistry))
    split = price.between_share(price.window_fitness(window), host_units(window.entities_start))
    out["host_fitness_between"] = split["share"] if split else None
    out["host_fitness_between_excess"] = split["excess"] if split else None
    end_deps = {i: capability.dependencies(e.genome, chemistry) for i, e in window.entities_end.items()}
    return out, pairs, d2, neighbour_pairs(window, radius, end_deps)


def analyse_run(run_dir, config, settings):
    """Analyses every observation window of a run. Returns (per-window rows, run summary)."""
    chemistry = capability.chemistry_of(config)
    resources = exchange_resources(config)
    rows, previous = [], None
    kept = [0, 0]
    base = [0, 0]
    for directory in data.windows(run_dir):
        window = data.read_window(directory)
        row, pairs, _, potential = analyse_window(window, chemistry, resources, settings)
        if previous is not None:
            alive = set(window.entities_end)
            old_pairs, old_potential = previous
            for total, (earlier, later) in zip((kept, base), ((old_pairs, pairs), (old_potential, pairs))):
                found, eligible = groups.retention(earlier, later, alive)
                total[0] += found
                total[1] += eligible
        previous = (pairs, potential)
        rows.append({"start": window.start, "end": window.end, **row})
    if not rows:
        raise ValueError(f"{run_dir} has no observation windows")
    summary = {key: _mean([r[key] for r in rows]) for key in rows[0] if key not in ("start", "end")}
    summary["partner_retention"] = kept[0] / kept[1] if kept[1] else None
    # Base rate: how often a pair of unlike neighbours is a mutual pair in the next window.
    summary["neighbour_retention"] = base[0] / base[1] if base[1] else None
    summary["windows"] = len(rows)
    return rows, summary
