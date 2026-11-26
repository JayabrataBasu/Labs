"""Hand-computed checks of the analysis formulas."""
import math
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "python"))

from evolab import assortment, capability, counterfactual, data, flux, groups, metrics, price  # noqa: E402

# resources: 0=E 1=F 2=A 3=B 4=C
E, F, A, B, C = range(5)
CHEM = [(A, B, 200000), (A, C, 200000), (B, E, 0), (C, F, 0)]


def genome(text):
    return data.parse_genome(text)


FULL = "uptake:2>2@always*60;convert:2>3@always*30;convert:2>4@always*30;uptake:3>3@always*30;uptake:4>4@always*30;convert:3>0@always*30;convert:4>1@always*30"
NEEDS_B = "uptake:2>2@always*60;convert:2>4@always*30;uptake:3>3@always*30;convert:3>0@always*30;convert:4>1@always*30"
NEEDS_C = "uptake:2>2@always*60;convert:2>3@always*30;uptake:4>4@always*30;convert:3>0@always*30;convert:4>1@always*30"


class Capability(unittest.TestCase):
    def test_parse_genome(self):
        g = genome("convert:2>3@1>=5*30;uptake:3>3@always*0")
        self.assertEqual((g[0].kind, g[0].a, g[0].b, g[0].sense, g[0].threshold, g[0].level), ("convert", 2, 3, 1, 5, 30))
        self.assertEqual((g[1].sense, g[1].level), (None, 0))
        self.assertEqual(genome(""), [])

    def test_dependence_needs_use_and_uptake_without_production(self):
        leaky = capability.leaky_reactions(CHEM)
        self.assertEqual(leaky, [(A, B), (A, C)])
        self.assertEqual(capability.dependencies(genome(FULL), CHEM), frozenset())
        self.assertEqual(capability.dependencies(genome(NEEDS_B), CHEM), frozenset({B}))
        self.assertEqual(capability.dependencies(genome(NEEDS_C), CHEM), frozenset({C}))
        # Lost the production step but also the uptake: it cannot be living on the product.
        self.assertEqual(capability.dependencies(genome(NEEDS_B.replace("uptake:3>3@always*30;", "")), CHEM), frozenset())
        # A gene with level 0 does nothing.
        self.assertEqual(capability.dependencies(genome(FULL.replace("convert:2>3@always*30", "convert:2>3@always*0")), CHEM), frozenset({B}))

    def test_summary_and_complementary_coexistence(self):
        pop = [genome(FULL)] * 6 + [genome(NEEDS_B)] * 2 + [genome(NEEDS_C)] * 2
        s = capability.summary(pop, CHEM, min_fraction=0.1)
        self.assertEqual((s["dependent_fraction"], s["dependent_on_3"], s["dependent_on_4"], s["complementary"]), (0.4, 0.2, 0.2, 1))
        self.assertEqual(capability.summary([genome(FULL)] * 9 + [genome(NEEDS_B)], CHEM, 0.05)["complementary"], 0)
        self.assertEqual(capability.summary([], CHEM, 0.05)["complementary"], 0)


class Assortment(unittest.TestCase):
    def test_makes_requires_the_substrate_uptake_too(self):
        self.assertEqual(capability.makes(genome(FULL), CHEM), frozenset({B, C}))
        self.assertEqual(capability.makes(genome(NEEDS_B), CHEM), frozenset({C}))
        self.assertEqual(capability.makes(genome(FULL.replace("uptake:2>2@always*60;", "")), CHEM), frozenset())

    def layout(self, rows):
        """One site per row of a 1-wide lattice; each row lists the genomes living there."""
        ents, i = [], 0
        for site, here in enumerate(rows):
            for g in here:
                ents.append(data.Entity(i, site, None, 0, (), tuple(genome(g))))
                i += 1
        return ents

    def measure(self, rows):
        return assortment.measure(self.layout(rows), CHEM, width=1, height=len(rows), wrap=False, radius=0, permutations=200, seed=5)

    def test_interleaved_partners_exceed_chance_and_segregated_fall_below(self):
        together = self.measure([[NEEDS_B, NEEDS_C]] * 8)
        self.assertEqual((together["reciprocal_fraction"], together["supplied_fraction"]), (1.0, 1.0))
        self.assertGreater(together["reciprocal_excess"], 0.3)
        apart = self.measure([[NEEDS_B, NEEDS_B], [NEEDS_C, NEEDS_C]] * 4)
        self.assertEqual(apart["reciprocal_fraction"], 0.0)
        self.assertLess(apart["reciprocal_excess"], -0.3)

    def test_one_way_supply_is_not_reciprocal_and_no_dependants_gives_none(self):
        one_way = self.measure([[NEEDS_B, FULL]] * 4)
        self.assertEqual((one_way["supplied_fraction"], one_way["reciprocal_fraction"]), (1.0, 0.0))
        none = self.measure([[FULL, FULL]] * 4)
        self.assertEqual((none["supplied_fraction"], none["reciprocal_excess"]), (None, None))


class Flux(unittest.TestCase):
    def test_attribution_is_proportional_and_bounded(self):
        # 3x1 lattice without wrap, radius 0: donors 1 (6 units) and 2 (2 units), taker 3 takes 4 at the same site.
        flows = [data.Flow(1, 0, B, 0, 6, 0), data.Flow(2, 0, B, 0, 2, 0), data.Flow(3, 0, B, 4, 0, 0)]
        edges = flux.attribute(flows, width=3, height=1, wrap=False, resources=[B], radius=0)
        self.assertAlmostEqual(edges[(1, 3)], 3.0)
        self.assertAlmostEqual(edges[(2, 3)], 1.0)
        # Taker wants more than was released: only what was released is attributed.
        flows = [data.Flow(1, 0, B, 0, 2, 0), data.Flow(3, 0, B, 10, 0, 0)]
        self.assertAlmostEqual(flux.attribute(flows, 3, 1, False, [B], 0)[(1, 3)], 2.0)

    def test_radius_spreads_releases_and_ignores_other_resources(self):
        flows = [data.Flow(1, 0, B, 0, 10, 0), data.Flow(2, 1, B, 10, 0, 0), data.Flow(3, 3, B, 10, 0, 0), data.Flow(1, 0, A, 0, 9, 0), data.Flow(2, 0, A, 9, 0, 0)]
        edges = flux.attribute(flows, width=5, height=1, wrap=False, resources=[B], radius=1)
        self.assertAlmostEqual(edges[(1, 2)], 5.0, msg="site 0 has neighbourhood {0, 1}: half reaches site 1")
        self.assertNotIn((1, 3), edges, "out of reach")
        wrapped = flux.attribute([data.Flow(1, 0, B, 0, 9, 0), data.Flow(2, 4, B, 9, 0, 0)], 5, 1, True, [B], 1)
        self.assertAlmostEqual(wrapped[(1, 2)], 3.0, msg="on a ring site 0 reaches sites 4, 0 and 1")

    def test_self_supply_is_reported_separately(self):
        flows = [data.Flow(1, 0, B, 4, 4, 0)]
        edges = flux.attribute(flows, 1, 1, False, [B], 0)
        self.assertAlmostEqual(edges[(1, 1)], 4.0)


class Groups(unittest.TestCase):
    # 1 <-> 2 reciprocal, 3 <-> 4 reciprocal, 5 -> 1 one way, 6 trades trivially with 4.
    EDGES = {(1, 2): 5.0, (2, 1): 5.0, (3, 4): 2.0, (4, 3): 2.0, (5, 1): 4.0, (6, 4): 0.01, (4, 6): 0.01, (1, 1): 9.0}
    INTAKE = {1: 9.0, 2: 5.0, 3: 2.0, 4: 2.1, 6: 1.0}

    def test_mutual_pairs_need_both_directions_above_the_share(self):
        pairs = groups.mutual_pairs(self.EDGES, self.INTAKE, min_share=0.1)
        self.assertEqual(sorted(pairs), [(1, 2), (3, 4)])

    def test_two_detectors_and_their_agreement(self):
        pairs = groups.mutual_pairs(self.EDGES, self.INTAKE, 0.1)
        components = groups.components(pairs)
        self.assertEqual(sorted(map(sorted, components)), [[1, 2], [3, 4]])
        labels = groups.label_propagation(pairs)
        self.assertEqual(sorted(map(sorted, labels)), [[1, 2], [3, 4]])
        self.assertEqual(groups.agreement(components, labels), 1.0)
        self.assertEqual(groups.agreement([{1, 2, 3, 4}], labels), 2 / 6)
        self.assertEqual(groups.agreement([], []), 1.0)
        big = [set(range(20000))]
        self.assertAlmostEqual(groups.agreement(big, [set(range(10000)), set(range(10000, 20000))]), (2 * 10000 * 9999 / 2) / (20000 * 19999 / 2))

    def test_label_propagation_splits_a_weak_bridge(self):
        pairs = {(a, b): 1.0 for a, b in [(1, 2), (1, 3), (2, 3), (4, 5), (4, 6), (5, 6)]}
        pairs[(3, 4)] = 0.01
        self.assertEqual(len(groups.components(pairs)), 1)
        self.assertEqual(sorted(map(sorted, groups.label_propagation(pairs))), [[1, 2, 3], [4, 5, 6]])

    def test_partner_retention_against_baseline(self):
        earlier, later = {(1, 2): 1.0, (3, 4): 1.0}, {(1, 2): 1.0}
        alive = {1, 2, 3, 4}
        self.assertEqual(groups.retention(earlier, later, alive), (1, 2))
        self.assertEqual(groups.retention(earlier, later, {1, 2}), (1, 1), "pairs with a dead member do not count")


class Metrics(unittest.TestCase):
    def test_encapsulation(self):
        edges = {(1, 2): 3.0, (2, 1): 3.0, (1, 9): 2.0, (8, 2): 4.0, (1, 1): 100.0}
        self.assertAlmostEqual(metrics.encapsulation({1, 2}, edges), 6 / 12)
        self.assertIsNone(metrics.encapsulation({7}, edges))

    def test_specialization_is_normalised_mutual_information(self):
        same = [[2.0, 2.0], [3.0, 3.0]]
        split = [[4.0, 0.0], [0.0, 4.0]]
        self.assertAlmostEqual(metrics.specialization(same), 0.0)
        self.assertAlmostEqual(metrics.specialization(split), 1.0)
        self.assertIsNone(metrics.specialization([[0.0, 0.0], [0.0, 0.0]]))


class Forks(unittest.TestCase):
    def test_pairs_are_reciprocal_same_site_and_disjoint(self):
        g = [tuple(genome(x)) for x in (NEEDS_B, NEEDS_C, NEEDS_B, FULL)]
        ents = [data.Entity(0, 0, None, 0, (), g[0]), data.Entity(1, 0, None, 0, (), g[1]), data.Entity(2, 0, None, 0, (), g[2]),
                data.Entity(3, 0, None, 0, (), g[3]), data.Entity(4, 1, None, 0, (), g[0]), data.Entity(5, 2, None, 0, (), g[1])]
        pairs = counterfactual.choose_pairs(ents, CHEM, 10, seed=1)
        self.assertEqual(len(pairs), 1, "only one disjoint reciprocal pair can be formed at site 0")
        self.assertIn(pairs[0], [(0, 1), (1, 2)])
        self.assertEqual(len(counterfactual.request(pairs)["sets"]), 3)

    def test_synergy_and_dependence_arithmetic(self):
        def member(alive, descendants):
            return {"alive": alive, "descendants": descendants}
        result = {"results": [{"name": "0:pair", "members": [member(True, 4), member(True, 2)]},
                              {"name": "0:first", "members": [member(False, 1)]}, {"name": "0:second", "members": [member(True, 0)]}]}
        s = counterfactual.score(result)
        self.assertEqual((s["pairs_tested"], s["synergy"], s["dependence"]), (1, 8 - 2, 3.0))
        self.assertEqual(counterfactual.score({"results": []})["synergy"], None)


class Price(unittest.TestCase):
    def test_partition_is_exact(self):
        w = {1: 2.0, 2: 0.0, 3: 3.0, 4: 1.0, 5: 1.0}
        z = {1: 1.0, 2: 0.0, 3: 2.0, 4: 2.0, 5: 0.0}
        parts = price.partition(w, z, [{1, 2}, {3, 4}])
        mean_w, mean_z = 7 / 5, 5 / 5
        total = sum((w[i] - mean_w) * (z[i] - mean_z) for i in w) / 5 / mean_w
        self.assertAlmostEqual(parts["selection"], total)
        self.assertAlmostEqual(parts["between"] + parts["within"], total)
        # group {1,2}: cov = ((2-1)(1-.5) + (0-1)(0-.5))/2 = 0.5, weight 2/5; {3,4}: cov = 0; singleton 5: 0.
        self.assertAlmostEqual(parts["within"], (0.5 * 2 / 5) / mean_w)

    def test_no_reproduction_gives_no_value(self):
        self.assertIsNone(price.partition({1: 0.0}, {1: 1.0}, []))


if __name__ == "__main__":
    unittest.main()
