"""Inference for registered reports: randomization tests, bootstrap intervals, Holm adjustment.

Runs (seeds) are the unit of replication. Every random procedure takes an explicit seed.
"""
import itertools
import math
import random
import statistics

EXACT_LIMIT = 200_000
_SLACK = 1e-12  # so that ties are counted despite rounding


def _require(*samples):
    if any(len(s) == 0 for s in samples):
        raise ValueError("a sample is empty")


def permutation_test(a, b, seed, resamples=100_000):
    """Two-sided randomization test of the difference in means between two groups of runs. Exact when
    the number of splits is small; otherwise sampled, counting the observed split once."""
    _require(a, b)
    pooled, n, total = list(a) + list(b), len(a), sum(a) + sum(b)

    def gap(part):
        return abs(part / n - (total - part) / (len(pooled) - n))

    observed = gap(sum(a)) - _SLACK
    if math.comb(len(pooled), n) <= EXACT_LIMIT:
        splits = [gap(sum(c)) >= observed for c in itertools.combinations(pooled, n)]
        return sum(splits) / len(splits)
    rng = random.Random(seed)
    hits = sum(gap(sum(rng.sample(pooled, n))) >= observed for _ in range(resamples))
    return (hits + 1) / (resamples + 1)


def sign_flip_test(values, seed, resamples=100_000):
    """Two-sided test that values are symmetric about zero (one value per run)."""
    _require(values)
    observed = abs(sum(values)) - _SLACK
    if 2 ** len(values) <= EXACT_LIMIT:
        hits = [abs(sum(v * s for v, s in zip(values, signs))) >= observed for signs in itertools.product((1, -1), repeat=len(values))]
        return sum(hits) / len(hits)
    rng = random.Random(seed)
    hits = sum(abs(sum(v if rng.random() < 0.5 else -v for v in values)) >= observed for _ in range(resamples))
    return (hits + 1) / (resamples + 1)


def bootstrap_difference(a, b, seed, resamples=10_000, level=0.95):
    """(mean(a) - mean(b), lower, upper): percentile bootstrap interval, resampling runs within groups."""
    _require(a, b)
    rng = random.Random(seed)
    draws = sorted(statistics.fmean(rng.choices(a, k=len(a))) - statistics.fmean(rng.choices(b, k=len(b))) for _ in range(resamples))
    cut = round((1 - level) / 2 * (resamples - 1))
    return statistics.fmean(a) - statistics.fmean(b), draws[cut], draws[resamples - 1 - cut]


def holm(p_values):
    """Holm step-down adjusted p-values, as {name: adjusted}."""
    out, running = {}, 0.0
    ordered = sorted(p_values.items(), key=lambda item: item[1])
    for i, (name, p) in enumerate(ordered):
        running = max(running, min(1.0, (len(ordered) - i) * p))
        out[name] = running
    return out
