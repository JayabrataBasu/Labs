"""Matching the neutral null (N3) to the demography of the intact world."""
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "python"))

from evolab import calibrate  # noqa: E402


def series(rows):
    return [{"tick": str(t), "entities": str(n), "births": str(b), "deaths": str(d)} for t, n, b, d in rows]


class Calibrate(unittest.TestCase):
    def test_rates_per_entity_and_tick_over_the_later_part(self):
        rows = series([(0, 10, 0, 0), (100, 100, 500, 100), (200, 100, 600, 200), (300, 300, 1000, 600)])
        # From tick 100: 500 deaths over (100+100)/2*100 + (100+300)/2*100 = 30000 entity-ticks.
        d = calibrate.demography(rows, from_tick=100)
        self.assertAlmostEqual(d["death_ppm"], 500 / 30000 * 1e6)
        self.assertAlmostEqual(d["birth_ppm"], 500 / 30000 * 1e6)
        self.assertAlmostEqual(d["entities"], 30000 / 200)
        with self.assertRaises(ValueError):
            calibrate.demography(rows, from_tick=300)

    def test_extra_death_rate_is_what_random_death_does_not_already_cover(self):
        cfg = {"rules": [{"id": "death.random", "params": {"probability_ppm": 5000}}, {"id": "death.starvation", "params": {}}]}
        self.assertEqual(calibrate.extra_death_ppm(cfg, 7200.4), 2200)
        self.assertEqual(calibrate.extra_death_ppm(cfg, 4000.0), 0)

    def test_birth_probability_is_found_by_bisection(self):
        tried = []

        def population(birth_ppm):
            tried.append(birth_ppm)
            return min(9000, birth_ppm / 2)

        found, reached = calibrate.search(population, target=4000, low=0, high=1_000_000, tolerance=0.01, steps=30)
        self.assertAlmostEqual(reached, 4000, delta=40)
        self.assertAlmostEqual(found, 8000, delta=80)
        self.assertLess(len(tried), 31)

    def test_unreachable_target_is_reported(self):
        with self.assertRaises(ValueError):
            calibrate.search(lambda p: min(100, p), target=4000, low=0, high=1_000_000, tolerance=0.01, steps=30)


if __name__ == "__main__":
    unittest.main()
