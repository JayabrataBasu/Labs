"""Runs an experiment: one `lab run` per (arm, sweep point, seed), in parallel processes."""
import csv
import json
import os
import subprocess
import tomllib
from concurrent.futures import ProcessPoolExecutor
from pathlib import Path

from . import spec as specmod
from . import toml_out


def lab_binary():
    path = os.environ.get("LAB_BIN")
    if path:
        return path
    here = Path(__file__).resolve().parents[2] / "target" / "release" / "lab"
    if here.exists():
        return str(here)
    raise FileNotFoundError("lab binary not found: run `cargo build --release` or set LAB_BIN")


def _run_one(job):
    binary, out, config_text, ticks, series_every, threads = job
    out = Path(out)
    out.mkdir(parents=True, exist_ok=True)
    (out / "config.toml").write_text(config_text)
    done = subprocess.run(
        [binary, "run", str(out / "config.toml"), "--ticks", str(ticks), "--threads", str(threads), "--out", str(out), "--series-every", str(series_every)],
        capture_output=True, text=True)
    if done.returncode != 0:
        return {"ok": False, "out": str(out), "error": done.stderr.strip()}
    manifest = json.loads((out / "manifest.json").read_text())
    return {"ok": True, "out": str(out), "final_sha256": manifest["final_sha256"], "git_dirty": manifest["git_dirty"],
            "entity_ticks_per_second": manifest["entity_ticks_per_second"]}


def run_experiment(spec_path, out_root, jobs, exploratory=False):
    spec, text, base_path = specmod.load(spec_path)
    if not exploratory:
        problems = specmod.verify_frozen(spec_path)
        if problems:
            raise SystemExit("refusing to run: " + "; ".join(problems) + "\n(use --exploratory for runs that are not part of a registered design)")
    base_cfg = tomllib.loads(base_path.read_text())
    runs = specmod.expand(spec, base_cfg)
    root = Path(out_root) / spec["name"]
    root.mkdir(parents=True, exist_ok=True)
    binary = lab_binary()
    version = subprocess.run([binary, "--version"], capture_output=True, text=True).stdout.strip()
    (root / "experiment.json").write_text(json.dumps({
        "name": spec["name"], "registered": not exploratory, "spec_text": text, "spec_sha256": specmod.sha256(text.encode()),
        "base_config": str(base_path), "lab_binary": binary, "lab_version": version, "n_runs": len(runs),
        "primary_outcome": spec["primary_outcome"], "analysis_plan": spec["analysis_plan"]}, indent=1) + "\n")
    work = [(binary, str(root / r["arm"] / r["point"] / f"seed-{r['seed']}"), toml_out.dumps(r["config"]), spec["ticks"],
             spec["series_every"], spec["threads"]) for r in runs]
    with ProcessPoolExecutor(max_workers=jobs) as pool:
        results = list(pool.map(_run_one, work))
    failed = [r for r in results if not r["ok"]]
    rows = []
    for run, result in zip(runs, results):
        if not result["ok"]:
            continue
        last = last_row(Path(result["out"]) / "series.csv")
        rows.append({"arm": run["arm"], "point": run["point"], "seed": run["seed"], "final_sha256": result["final_sha256"], **last})
    if rows:
        with open(root / "summary.csv", "w", newline="") as f:
            w = csv.DictWriter(f, fieldnames=list(rows[0]))
            w.writeheader()
            w.writerows(rows)
    if failed:
        raise SystemExit(f"{len(failed)} of {len(runs)} runs failed; first: {failed[0]['out']}: {failed[0]['error']}")
    return root


def last_row(series_csv):
    with open(series_csv, newline="") as f:
        rows = list(csv.DictReader(f))
    return rows[-1]
