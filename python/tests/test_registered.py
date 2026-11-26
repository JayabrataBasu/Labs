"""The hypotheses a registered spec declares, evaluated from its results."""
import contextlib
import io
import json
import shutil
import sys
import tempfile
import tomllib
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "python"))

from evolab import registered, report  # noqa: E402
from evolab.__main__ import main  # noqa: E402

SPEC = '''name = "t"
[inference]
seed = 7
resamples = 5000
bootstrap_resamples = 2000
alpha = 0.05
[[arms]]
name = "intact"
transforms = []
[[arms]]
name = "null"
transforms = []
[[hypotheses]]
name = "H1"
test = "difference"
metric = "m"
arm = "intact"
against = "null"
smallest_effect = 2
[[hypotheses]]
name = "H2"
test = "against_zero"
metric = "z"
arm = "intact"
smallest_effect = 1
'''


class Hypotheses(unittest.TestCase):
    def results(self, rows, spec=SPEC):
        d = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, d, True)
        (d / "experiment.json").write_text(json.dumps({"name": "t", "registered": True, "spec_text": spec, "spec_sha256": "ab" * 32,
                                                       "lab_version": "lab", "n_runs": len(rows), "primary_outcome": "m"}))
        (d / "summary.csv").write_text("\n".join(["arm,point,seed,m,z,final_sha256"] + [",".join(map(str, r)) + ",ab" for r in rows]) + "\n")
        return d

    def separated(self):
        return self.results([("intact", "base", s, 9 + s, -s) for s in range(1, 6)] + [("null", "base", s, s, 0) for s in range(1, 6)])

    def test_estimates_adjusted_p_values_and_verdicts(self):
        h1, h2 = registered.evaluate(self.separated())
        self.assertEqual((h1["name"], h1["n"], h1["n_against"], h1["estimate"]), ("H1", 5, 5, 9.0))
        self.assertAlmostEqual(h1["p"], 2 / 252)
        self.assertAlmostEqual(h1["p_adjusted"], 4 / 252)
        self.assertEqual(h1["verdict"], "supported")
        self.assertTrue(h1["low"] <= 9 <= h1["high"])
        self.assertEqual((h2["estimate"], h2["n_against"]), (-3.0, None))
        self.assertAlmostEqual(h2["p"], 2 / 32)
        self.assertEqual(h2["verdict"], "inconclusive")

    def test_a_precise_small_difference_is_reported_as_below_the_threshold(self):
        rows = [(a, "base", s, 10 + s % 2, 0.1 if s % 2 else -0.1) for a in ("intact", "null") for s in range(1, 9)]
        h1, h2 = registered.evaluate(self.results(rows))
        self.assertEqual(h1["verdict"], "smaller than the threshold")
        self.assertEqual(h2["verdict"], "smaller than the threshold")

    def test_missing_values_are_counted_and_a_constant_outcome_decides_nothing(self):
        rows = [("intact", "base", s, 0, "") for s in range(1, 6)] + [("null", "base", s, 0, "") for s in range(1, 6)]
        h1, h2 = registered.evaluate(self.results(rows))
        self.assertEqual(h1["verdict"], "inconclusive: the outcome does not vary")
        self.assertEqual((h2["n"], h2["missing"], h2["verdict"], h2["p"]), (0, 5, "not evaluable", None))

    def test_malformed_declarations_are_errors(self):
        for old, new in (('against = "null"', 'against = "nowhere"'), ('smallest_effect = 2', ''), ('test = "difference"', 'test = "t"'),
                         ('alpha = 0.05\n', ''), ('smallest_effect = 1', 'smallest_effect = 0')):
            with self.assertRaises(ValueError, msg=new):
                registered.check(tomllib.loads(SPEC.replace(old, new)))
        registered.check(tomllib.loads(SPEC))
        registered.check({"arms": []})

    def test_the_command_prints_each_verdict_and_saves_the_numbers(self):
        d = self.separated()
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            self.assertEqual(main(["infer", str(d)]), 0)
        self.assertRegex(out.getvalue(), r"H1 .*estimate 9 .*p_adjusted 0\.0159.*supported")
        self.assertEqual(json.loads((d / "inference.json").read_text())[0]["verdict"], "supported")

    def test_small_p_values_are_not_shown_as_zero(self):
        self.assertEqual(report.fmt_p(1 / 100001), "1e-05")
        self.assertEqual(report.fmt_p(0.0159), "0.0159")

    def test_the_report_shows_the_registered_tests(self):
        page = report.build(self.separated())
        self.assertIn("Registered tests", page)
        self.assertIn("supported", page)


if __name__ == "__main__":
    unittest.main()
