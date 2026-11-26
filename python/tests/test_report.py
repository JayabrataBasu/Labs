"""The self-contained HTML report of an experiment."""
import json
import re
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "python"))

from evolab import report  # noqa: E402

HOSTILE = "<script>alert(1)</script>"


class Report(unittest.TestCase):
    def setUp(self):
        self.d = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.d, True)
        (self.d / "experiment.json").write_text(json.dumps({
            "name": "demo", "registered": False, "spec_text": 'name = "demo"\naim = "Does it move?"\n', "spec_sha256": "ab" * 32,
            "lab_version": "lab 0.1", "n_runs": 4, "primary_outcome": "score", "analysis_plan": "Look " + HOSTILE}))
        rows = ["arm,point,seed,final_sha256,tick,entities,score,blank,label"]
        for arm, values in (("intact", (1.0, 3.0)), (HOSTILE, (5.0, 5.0))):
            for seed, v in zip((1, 2), values):
                rows.append(f"{arm},base,{seed},{'cd' * 32},20,{int(10 * v)},{v},,text")
                run = self.d / arm.replace("/", "_") / "base" / f"seed-{seed}"
                run.mkdir(parents=True)
                (run / "series.csv").write_text(f"tick,entities\n0,5\n10,{seed * 10}\n20,{int(10 * v)}\n")
        (self.d / "summary.csv").write_text("\n".join(rows) + "\n")

    def test_page_is_self_contained_escaped_and_states_its_status(self):
        path = report.write(self.d)
        page = path.read_text()
        self.assertEqual(path, self.d / "report.html")
        self.assertIn("demo", page)
        self.assertIn("Exploratory", page)
        self.assertIn("Does it move?", page)
        self.assertNotIn(HOSTILE, page)
        self.assertIn("&lt;script&gt;", page)
        self.assertNotRegex(page, r"(src|href)\s*=\s*[\"']?https?:")
        registered = json.loads((self.d / "experiment.json").read_text()) | {"registered": True}
        (self.d / "experiment.json").write_text(json.dumps(registered))
        self.assertIn("Registered", report.write(self.d).read_text())

    def test_one_dot_per_run_placed_by_value_and_only_numeric_columns_are_charted(self):
        page = report.build(self.d)
        chart = re.search(r'<section data-metric="score">.*?</section>', page, re.S).group(0)
        dots = [float(x) for x in re.findall(r'<circle class="dot" cx="([\d.]+)"', chart)]
        self.assertEqual(len(dots), 4)
        # intact has 1 and 3, the other arm 5 and 5: positions are in the same order and evenly spaced
        low, mid, high, high2 = sorted(dots)
        self.assertEqual(high, high2)
        self.assertAlmostEqual(mid - low, high - mid, places=1)
        metrics = re.findall(r'data-metric="([^"]+)"', page)
        self.assertEqual(metrics[0], "score", "the primary outcome comes first")
        self.assertIn("entities", metrics)
        self.assertNotIn("blank", metrics)
        self.assertNotIn("label", metrics)
        self.assertNotIn("final_sha256", metrics)

    def test_time_series_show_the_mean_and_range_over_seeds(self):
        self.assertEqual(report.series_summary([[(0, 5.0), (10, 10.0)], [(0, 5.0), (10, 30.0)]]), [(0, 5.0, 5.0, 5.0), (10, 10.0, 20.0, 30.0)])
        self.assertIn("<polygon", report.build(self.d))

    def test_cells_are_ordered_by_number_not_by_spelling(self):
        cells = [("b", "leak=500000"), ("a", "leak=500000_move=9"), ("a", "leak=50000_move=10000"), ("a", "leak=50000_move=900")]
        self.assertEqual(sorted(cells, key=report.natural), [cells[3], cells[2], cells[1], cells[0]])

    def test_axis_covers_the_values_with_round_ends(self):
        self.assertEqual(report.axis([1.0, 3.0, 5.0]), (0.0, 5.0))
        self.assertEqual(report.axis([-0.07, -0.01]), (-0.1, 0.0))
        self.assertEqual(report.axis([2.0, 2.0]), (0.0, 2.0))
        self.assertEqual(report.axis([0.0]), (0.0, 1.0))
        lo, hi = report.axis([17.0, 11800.0])
        self.assertTrue(lo <= 17 and hi >= 11800 and hi <= 20000)


if __name__ == "__main__":
    unittest.main()
