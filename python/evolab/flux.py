"""Who fed whom, estimated by local-pool attribution (DR-021).

The kernel records what each entity released to a site and took from a site; matter on a site is
not tagged with its source. The estimator assumes that what is released at a site is available
evenly over that site's neighbourhood (radius r), and that takers at a site draw on all donors in
the local pool in proportion to what each put there. At every site the attributed amount is
min(pool, takes), so no donor is credited with more than it released and no taker with more than
it took. It is an ESTIMATE: diffusion over longer distances, decay and timing within the window
are ignored.
"""
from collections import defaultdict


def neighbourhood(site, width, height, wrap, radius):
    """Sites within `radius` steps (4-neighbour distance) of `site`, itself included."""
    x, y = site % width, site // width
    found = set()
    for dy in range(-radius, radius + 1):
        for dx in range(-(radius - abs(dy)), radius - abs(dy) + 1):
            nx, ny = x + dx, y + dy
            if wrap:
                nx, ny = nx % width, ny % height
            if 0 <= nx < width and 0 <= ny < height:
                found.add(ny * width + nx)
    return sorted(found)


def attribute(flows, width, height, wrap, resources, radius):
    """{(donor, taker): amount} over the given exchange resources. Donor == taker is self-supply."""
    wanted = set(resources)
    pool = defaultdict(lambda: defaultdict(float))  # (site, resource) -> donor -> amount
    takes = defaultdict(list)  # (site, resource) -> [(taker, amount)]
    cache = {}
    for f in flows:
        if f.resource not in wanted:
            continue
        if f.released:
            if f.site not in cache:
                cache[f.site] = neighbourhood(f.site, width, height, wrap, radius)
            share = f.released / len(cache[f.site])
            for u in cache[f.site]:
                pool[(u, f.resource)][f.entity] += share
        if f.taken:
            takes[(f.site, f.resource)].append((f.entity, f.taken))
    edges = defaultdict(float)
    for key, takers in takes.items():
        donors = pool.get(key)
        if not donors:
            continue
        available = sum(donors.values())
        demanded = sum(amount for _, amount in takers)
        scale = min(available, demanded) / (available * demanded)
        for taker, amount in takers:
            for donor, given in donors.items():
                edges[(donor, taker)] += given * amount * scale
    return dict(edges)


def intake(flows, resources):
    """{entity: total amount of the exchange resources it took up}, from any source."""
    total = defaultdict(float)
    for f in flows:
        if f.resource in resources and f.taken:
            total[f.entity] += f.taken
    return dict(total)
