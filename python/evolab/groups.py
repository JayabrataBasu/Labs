"""Detecting mutual exchange and groups built from it. Two detectors; neither presumes its answer.

An entity i *materially supplies* j when the estimated flux i -> j is at least `min_share` of
everything j took up of the exchange resources. A **mutual pair** supplies each other.
Detector D1: connected components of the mutual-pair graph.
Detector D2: weighted label propagation on the same graph, which can split weakly bridged parts.
Their agreement is reported with every result.
"""
from collections import Counter, defaultdict


def mutual_pairs(edges, intake, min_share):
    """{(i, j): weight} with i < j; weight is the smaller of the two directed fluxes."""
    pairs = {}
    for (i, j), amount in edges.items():
        if i >= j:
            continue
        back = edges.get((j, i), 0.0)
        if amount > 0 and back > 0 and amount >= min_share * intake.get(j, 0) and back >= min_share * intake.get(i, 0):
            pairs[(i, j)] = min(amount, back)
    return pairs


def _adjacency(pairs):
    graph = defaultdict(dict)
    for (i, j), w in pairs.items():
        graph[i][j] = w
        graph[j][i] = w
    return graph


def components(pairs):
    graph = _adjacency(pairs)
    seen, out = set(), []
    for start in sorted(graph):
        if start in seen:
            continue
        stack, group = [start], set()
        seen.add(start)
        while stack:
            node = stack.pop()
            group.add(node)
            for other in graph[node]:
                if other not in seen:
                    seen.add(other)
                    stack.append(other)
        out.append(group)
    return out


def label_propagation(pairs, max_rounds=100):
    """Deterministic: nodes visited in id order; a node keeps its label unless another is strictly
    heavier among its neighbours; ties go to the smallest label."""
    graph = _adjacency(pairs)
    label = {n: n for n in graph}
    for _ in range(max_rounds):
        changed = False
        for node in sorted(graph):
            weight = defaultdict(float)
            for other, w in graph[node].items():
                weight[label[other]] += w
            best = min(weight, key=lambda l: (-weight[l], l))
            if weight[best] > weight.get(label[node], 0.0) + 1e-12:
                label[node], changed = best, True
        if not changed:
            break
    out = defaultdict(set)
    for node, l in label.items():
        out[l].add(node)
    return [g for _, g in sorted(out.items()) if len(g) > 1]


def agreement(first, second):
    """Jaccard index of the sets of co-grouped pairs under two partitions (1 = identical).
    Counted from group overlaps, so large groups cost nothing extra."""
    def pairs(n):
        return n * (n - 1) // 2
    where = {member: k for k, g in enumerate(second) for member in g}
    shared = sum(pairs(n) for g in first for n in Counter(where[m] for m in g if m in where).values())
    a, b = sum(pairs(len(g)) for g in first), sum(pairs(len(g)) for g in second)
    union = a + b - shared
    return shared / union if union else 1.0


def retention(earlier, later, alive_later):
    """(pairs still present, pairs whose two members are both alive) from `earlier` to `later`."""
    eligible = [p for p in earlier if p[0] in alive_later and p[1] in alive_later]
    return sum(p in later for p in eligible), len(eligible)
