"""The inferential tools of registered reports, checked against hand-computed values."""
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "python"))

from evolab import stats  # noqa: E402


class Permutation(unittest.TestCase):
    def test_exact_two_sample_test(self):
        # 3 vs 3, completely separated: only the observed split and its mirror are this extreme.
        self.assertAlmostEqual(stats.permutation_test([4, 5, 6], [1, 2, 3], seed=1), 2 / 20)
        self.assertEqual(stats.permutation_test([1, 1, 1], [1, 1, 1], seed=1), 1.0)
        self.assertEqual(stats.permutation_test([1, 2, 3], [4, 5, 6], seed=1), stats.permutation_test([4, 5, 6], [1, 2, 3], seed=1))

    def test_large_samples_are_sampled_reproducibly_and_never_report_zero(self):
        a, b = [x + 100 for x in range(20)], list(range(20))
        p = stats.permutation_test(a, b, seed=3, resamples=2000)
        self.assertEqual(p, stats.permutation_test(a, b, seed=3, resamples=2000))
        self.assertAlmostEqual(p, 1 / 2001)

    def test_sign_flip_test_against_zero(self):
        # All five negative: 2 of the 32 sign patterns are as far from zero.
        self.assertAlmostEqual(stats.sign_flip_test([-1, -2, -3, -4, -5], seed=1), 2 / 32)
        self.assertEqual(stats.sign_flip_test([-1, 1], seed=1), 1.0)
        self.assertEqual(stats.sign_flip_test([0, 0, 0], seed=1), 1.0)

    def test_empty_samples_are_an_error(self):
        for bad in (lambda: stats.permutation_test([], [1], seed=1), lambda: stats.sign_flip_test([], seed=1),
                    lambda: stats.bootstrap_difference([1], [], seed=1)):
            with self.assertRaises(ValueError):
                bad()


class Bootstrap(unittest.TestCase):
    def test_interval_of_a_difference_in_means(self):
        diff, low, high = stats.bootstrap_difference([5, 5, 5], [2, 2, 2], seed=1)
        self.assertEqual((diff, low, high), (3.0, 3.0, 3.0))
        diff, low, high = stats.bootstrap_difference([1, 2, 3, 4, 5, 6], [0, 1, 0, 1, 0, 1], seed=1)
        self.assertAlmostEqual(diff, 3.0)
        self.assertTrue(1.0 < low < 3.0 < high < 5.0, (low, high))
        self.assertEqual((diff, low, high), stats.bootstrap_difference([1, 2, 3, 4, 5, 6], [0, 1, 0, 1, 0, 1], seed=1))


class Holm(unittest.TestCase):
    def test_step_down_adjustment(self):
        adjusted = stats.holm({"a": 0.01, "b": 0.04, "c": 0.03})
        self.assertEqual({k: round(v, 6) for k, v in adjusted.items()}, {"a": 0.03, "c": 0.06, "b": 0.06})
        self.assertEqual(stats.holm({"a": 0.9, "b": 0.8}), {"b": 1.0, "a": 1.0})


if __name__ == "__main__":
    unittest.main()
