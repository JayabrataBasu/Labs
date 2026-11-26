import json
import os
import shutil
import subprocess
import sys
import tempfile
import tomllib
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "python"))

from evolab import analysis, runner, spec, toml_out, transforms  # noqa: E402

CROSSFEED = tomllib.loads((ROOT / "configs" / "crossfeed.toml").read_text())
LAB = Path(os.environ.get("LAB_BIN") or ROOT / "target" / "release" / "lab")


class TomlWriter(unittest.TestCase):
    def test_roundtrips_real_configs(self):
        for name in sorted(p.name for p in (ROOT / "configs").glob("*.toml")):
            cfg = tomllib.loads((ROOT / "configs" / name).read_text())
            self.assertEqual(tomllib.loads(toml_out.dumps(cfg)), cfg, name)

    def test_rejects_floats_and_escapes_strings(self):
        with self.assertRaises(TypeError):
            toml_out.dumps({"x": 1.5})
        cfg = {"name": 'a "quoted"\\name'}
        self.assertEqual(tomllib.loads(toml_out.dumps(cfg)), cfg)


class Transforms(unittest.TestCase):
    def rule(self, cfg, id_):
        return [r for r in cfg["rules"] if r["id"] == id_]

    def test_no_secretion_changes_only_that_switch(self):
        out = transforms.apply(CROSSFEED, [{"name": "null.no_secretion"}])
        self.assertEqual(self.rule(out, "metabolism.genes")[0]["params"]["release_to"], "sink")
        self.assertEqual(self.rule(CROSSFEED, "metabolism.genes")[0]["params"]["release_to"], "site", "the base config is not modified")
        restored = json.loads(json.dumps(out))
        self.rule(restored, "metabolism.genes")[0]["params"]["release_to"] = "site"
        self.assertEqual(restored, CROSSFEED)

    def test_well_mixed_stops_walking_and_scatters(self):
        out = transforms.apply(CROSSFEED, [{"name": "null.well_mixed"}])
        self.assertEqual(self.rule(out, "move.random")[0]["params"]["probability_ppm"], 0)
        mix = self.rule(out, "mix.relocate")
        self.assertEqual(mix[0]["params"]["probability_ppm"], 1_000_000)
        streams = [r["stream"] for r in out["rules"]]
        self.assertEqual(len(streams), len(set(streams)), "the new rule gets an unused stream")

    def test_neutral_needs_explicit_rates_and_removes_selection(self):
        with self.assertRaises(ValueError):
            transforms.apply(CROSSFEED, [{"name": "null.neutral"}])
        out = transforms.apply(CROSSFEED, [{"name": "null.neutral", "birth_ppm": 30000, "death_ppm": 29000}])
        ids = [r["id"] for r in out["rules"]]
        self.assertNotIn("reproduction.division", ids)
        self.assertNotIn("death.starvation", ids)
        self.assertEqual(self.rule(out, "reproduction.random")[0]["params"]["probability_ppm"], 30000)
        self.assertEqual(self.rule(out, "death.random")[0]["params"]["probability_ppm"], 29000)
        self.assertEqual(self.rule(out, "reproduction.random")[0]["params"]["mutation"], self.rule(CROSSFEED, "reproduction.division")[0]["params"]["mutation"])

    def test_no_mutation_zeroes_every_rate_but_keeps_the_cap(self):
        m = self.rule(transforms.apply(CROSSFEED, [{"name": "null.no_mutation"}]), "reproduction.division")[0]["params"]["mutation"]
        self.assertEqual([m[k] for k in ("point_ppm", "retarget_ppm", "duplicate_ppm", "delete_ppm")], [0, 0, 0, 0])
        self.assertEqual(m["max_genes"], 16)

    def test_no_recycling_touches_every_death_route_and_n1_is_the_composition(self):
        out = transforms.apply(CROSSFEED, [{"name": "null.no_recycling"}])
        self.assertFalse(self.rule(out, "death.starvation")[0]["params"]["recycle"])
        self.assertTrue(self.rule(CROSSFEED, "death.starvation")[0]["params"]["recycle"])
        both = transforms.apply(CROSSFEED, [{"name": "null.no_secretion"}, {"name": "null.no_recycling"}])
        self.assertEqual(self.rule(both, "metabolism.genes")[0]["params"]["release_to"], "sink")
        self.assertFalse(self.rule(both, "death.starvation")[0]["params"]["recycle"])
        bare = {**CROSSFEED, "rules": [r for r in CROSSFEED["rules"] if r["id"] != "death.starvation"]}
        with self.assertRaises(ValueError):
            transforms.apply(bare, [{"name": "null.no_recycling"}])

    def test_neutral_death_inherits_the_recycling_setting(self):
        silent = transforms.apply(CROSSFEED, [{"name": "null.no_recycling"}, {"name": "null.neutral", "birth_ppm": 1, "death_ppm": 1}])
        self.assertFalse(self.rule(silent, "death.random")[0]["params"]["recycle"])

    def test_unknown_transform_and_missing_rule_are_errors(self):
        with self.assertRaises(ValueError):
            transforms.apply(CROSSFEED, [{"name": "null.nonsense"}])
        bare = {**CROSSFEED, "rules": [r for r in CROSSFEED["rules"] if r["id"] != "metabolism.genes"]}
        with self.assertRaises(ValueError):
            transforms.apply(bare, [{"name": "null.no_secretion"}])


class Specs(unittest.TestCase):
    def make(self, extra="", seeds="[1001, 1002, 1003]"):
        d = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, d, True)
        shutil.copy(ROOT / "configs" / "crossfeed.toml", d / "base.toml")
        text = f'''name = "t"
base_config = "base.toml"
ticks = 10
series_every = 5
threads = 1
seeds = {seeds}
primary_outcome = "entities"
aim = "test"
estimand = "test"
analysis_plan = "test"
[[arms]]
name = "intact"
transforms = []
[[arms]]
name = "N1_x"
transforms = [{{ name = "null.no_secretion" }}]
{extra}'''
        (d / "spec.toml").write_text(text)
        return d / "spec.toml"

    def test_expansion_is_the_full_factorial(self):
        path = self.make('[[sweeps]]\npath = "rules.5.params.divisor"\nvalues = [10, 20]\n[[sweeps]]\npath = "init.founders.0.count"\nvalues = [100, 200, 300]\n')
        s, _, base = spec.load(path)
        runs = spec.expand(s, tomllib.loads(base.read_text()))
        self.assertEqual(len(runs), 2 * 2 * 3 * 3)
        first = runs[0]
        self.assertEqual(first["config"]["init"]["founders"][0]["count"], 100)
        self.assertEqual([r["stream"] for r in first["config"]["rules"] if r["id"] == "decay.fraction"][0], 4)
        self.assertEqual(sorted({r["seed"] for r in runs}), [1001, 1002, 1003])
        self.assertTrue(all(r["config"]["seed"] == r["seed"] for r in runs))

    def test_sweeps_may_only_change_existing_settings(self):
        path = self.make('[[sweeps]]\npath = "rules.5.params.no_such_key"\nvalues = [1]\n')
        s, _, base = spec.load(path)
        with self.assertRaises(ValueError):
            spec.expand(s, tomllib.loads(base.read_text()))
        path = self.make('[[sweeps]]\npath = "rules.99.params.divisor"\nvalues = [1]\n')
        s, _, base = spec.load(path)
        with self.assertRaises(ValueError):
            spec.expand(s, tomllib.loads(base.read_text()))

    def test_registered_specs_cannot_use_exploratory_seeds(self):
        path = self.make(seeds="[1, 2, 3]")
        with self.assertRaises(ValueError) as caught:
            spec.freeze(path)
        self.assertIn("reserved for exploration", str(caught.exception))
        self.assertFalse(spec.frozen_path(path).exists())
        spec.load(path)  # loading (exploratory use) is still allowed

    def test_sweep_paths_reach_into_lists(self):
        cfg = json.loads(json.dumps(CROSSFEED))
        spec.set_path(cfg, "rules.6.params.reactions.0.leak_ppm", 123)
        self.assertEqual([r for r in cfg["rules"] if r["stream"] == 6][0]["params"]["reactions"][0]["leak_ppm"], 123)
        for bad in ("rules.6.params.reactions.9.leak_ppm", "init.founders.3.count", "rules.6.params.reactions.x.leak_ppm"):
            with self.assertRaises(ValueError):
                spec.set_path(cfg, bad, 1)

    def test_missing_fields_and_duplicate_seeds_are_errors(self):
        path = self.make()
        text = path.read_text().replace('analysis_plan = "test"\n', "")
        path.write_text(text)
        with self.assertRaises(ValueError):
            spec.load(path)
        with self.assertRaises(ValueError):
            spec.load(self.make(seeds="[1001, 1001]"))

    def test_freezing_detects_every_kind_of_change(self):
        path = self.make()
        self.assertTrue(spec.verify_frozen(path), "an unfrozen spec is refused")
        spec.freeze(path)
        self.assertEqual(spec.verify_frozen(path), [])
        original = path.read_text()
        path.write_text(original.replace("ticks = 10", "ticks = 11"))
        self.assertIn("the spec has changed since it was frozen", spec.verify_frozen(path))
        path.write_text(original)
        self.assertEqual(spec.verify_frozen(path), [])
        base = path.parent / "base.toml"
        base.write_text(base.read_text().replace("amount = 40", "amount = 41"))
        self.assertIn("the base config has changed since the spec was frozen", spec.verify_frozen(path))


class Analysis(unittest.TestCase):
    rows = [{"arm": a, "point": "base", "seed": str(s), "entities": str(v)}
            for a, vals in {"intact": [10, 20, 30, 40, 50], "N1_x": [5, 5, 5, 5, 5]}.items() for s, v in enumerate(vals)]

    def test_describe(self):
        d = analysis.describe([1, 2, 3, 4, 5])
        self.assertEqual((d["n"], d["mean"], d["median"], d["min"], d["max"], d["q1"], d["q3"]), (5, 3, 3, 1, 5, 2, 4))
        self.assertAlmostEqual(d["sd"], 1.5811388, places=6)
        self.assertEqual(analysis.describe([7])["sd"], 0.0)

    def test_construct_check_flags_the_failure_modes(self):
        ok = analysis.construct_check(self.rows, "entities")
        self.assertTrue(all(o for o, _ in ok) is False, "the constant N1 arm is reported")
        self.assertTrue(any("identical across all seeds" in m for _, m in ok))
        constant = [{**r, "entities": "7"} for r in self.rows]
        self.assertFalse(analysis.construct_check(constant, "entities")[0][0])
        few = [r for r in self.rows if r["seed"] in ("0", "1")]
        self.assertFalse(dict((m.split(":")[0], o) for o, m in analysis.construct_check(few, "entities"))["seeds per cell"])
        no_null = [r for r in self.rows if r["arm"] == "intact"]
        self.assertTrue(any(not o and "null-world" in m for o, m in analysis.construct_check(no_null, "entities")))


@unittest.skipUnless(LAB.exists(), "needs `cargo build --release`")
class EndToEnd(unittest.TestCase):
    def test_run_is_reproducible_and_null_world_differs(self):
        d = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, d, True)
        cfg = json.loads(json.dumps(CROSSFEED))
        cfg["world"]["width"] = cfg["world"]["height"] = 16
        cfg["init"]["founders"][0]["count"] = 80
        (d / "base.toml").write_text(toml_out.dumps(cfg))
        (d / "spec.toml").write_text('''name = "e2e"
base_config = "base.toml"
ticks = 150
series_every = 3
threads = 1
seeds = [1001, 1002]
primary_outcome = "entities"
aim = "test"
estimand = "test"
analysis_plan = "test"
[[arms]]
name = "intact"
transforms = []
[[arms]]
name = "N1_no_interaction"
transforms = [{ name = "null.no_secretion" }, { name = "null.no_recycling" }]
''')
        os.environ["LAB_BIN"] = str(LAB)
        self.addCleanup(os.environ.pop, "LAB_BIN", None)
        with self.assertRaises(SystemExit):
            runner.run_experiment(d / "spec.toml", d / "out", 2)  # unfrozen: refused
        spec.freeze(d / "spec.toml")
        first = runner.run_experiment(d / "spec.toml", d / "out1", 2)
        second = runner.run_experiment(d / "spec.toml", d / "out2", 3)
        a, b = analysis.load_summary(first), analysis.load_summary(second)
        self.assertEqual(len(a), 4)
        self.assertEqual([r["final_sha256"] for r in a], [r["final_sha256"] for r in b], "same spec, same hashes, whatever the parallelism")
        self.assertEqual(len({r["final_sha256"] for r in a}), 4, "arms and seeds all differ")
        meta = json.loads((first / "experiment.json").read_text())
        self.assertTrue(meta["registered"])

        n1 = (first / "N1_no_interaction" / "base" / "seed-1001" / "series.csv").read_text().splitlines()
        self.assertGreater(len(n1), 10)
        self.assertNotEqual(by_arm_hash(a, "intact"), by_arm_hash(a, "N1_no_interaction"), "removing the channels changes the trajectory")

        def site_b(arm):
            lines = (first / arm / "base" / "seed-1001" / "series.csv").read_text().splitlines()
            col = lines[0].split(",").index("site_B")
            return [int(line.split(",")[col]) for line in lines[1:]]

        # B is created only by conversion inside entities and reaches a site only by secretion or by an entity's death.
        # With both switched off (the full N1), no B can ever be on a site.
        self.assertEqual(set(site_b("N1_no_interaction")), {0}, "full N1: no transfer of B between entities")
        self.assertGreater(max(site_b("intact")), 0, "the intact world does put B on sites")


def by_arm_hash(rows, arm):
    return [r["final_sha256"] for r in rows if r["arm"] == arm and r["seed"] == "1001"][0]


if __name__ == "__main__":
    unittest.main()
