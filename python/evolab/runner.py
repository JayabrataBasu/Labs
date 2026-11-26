"""Runs an experiment: one `lab run` per (arm, sweep point, seed), in parallel processes."""
import csv
import json
import os
import shutil
import subprocess
import tomllib
from concurrent.futures import ProcessPoolExecutor
from pathlib import Path

from . import capability, counterfactual, data, metrics, toml_out
from . import spec as specmod


def lab_binary():
    path = os.environ.get("LAB_BIN")
    if path:
        return path
    release = Path(__file__).resolve().parents[2] / "target" / "release"
    for name in ("lab", "lab.exe"):
        if (release / name).exists():
            return str(release / name)
    raise FileNotFoundError("lab binary not found: run `cargo build --release` or set LAB_BIN")


def _run_one(job):
    binary, out, config_text, ticks, series_every, threads, observe = job
    out = Path(out)
    out.mkdir(parents=True, exist_ok=True)
    (out / "config.toml").write_text(config_text)
    command = [binary, "run", str(out / "config.toml"), "--ticks", str(ticks), "--threads", str(threads), "--out", str(out), "--series-every", str(series_every)]
    if observe:
        command += ["--observe-window", str(observe["window"]), "--observe-count", str(observe["count"])]
    done = subprocess.run(command, capture_output=True, text=True)
    if done.returncode != 0:
        return {"ok": False, "out": str(out), "error": done.stderr.strip()}
    manifest = json.loads((out / "manifest.json").read_text())
    result = {"ok": True, "out": str(out), "final_sha256": manifest["final_sha256"], "git_dirty": manifest["git_dirty"],
              "entity_ticks_per_second": manifest["entity_ticks_per_second"], "measures": {}}
    if observe:
        try:
            result["measures"] = _analyse(binary, out, tomllib.loads(config_text), ticks, observe)
        except Exception as error:  # a failed analysis is a failed run, reported with the others
            return {"ok": False, "out": str(out), "error": f"analysis failed: {error!r}"}
    return result


def _analyse(binary, out, config, ticks, observe):
    rows, summary = metrics.analyse_run(out, config, observe)
    if observe["counterfactual_pairs"]:
        last = data.read_window(data.windows(out)[-1])
        summary.update(counterfactual.run(binary, out / f"ckpt-{ticks}.bin", out / "config.toml", list(last.entities_end.values()),
                                          capability.chemistry_of(config), observe["counterfactual_pairs"],
                                          observe["counterfactual_ticks"], observe["shuffle_seed"], out))
    write_rows(out / "windows.csv", rows)
    (out / "metrics.json").write_text(json.dumps(summary, indent=1) + "\n")
    if not observe["keep_raw"]:
        shutil.rmtree(out / "obs")
    return summary


def write_rows(path, rows):
    keys = list(dict.fromkeys(k for row in rows for k in row))
    with open(path, "w", newline="") as f:
        w = csv.DictWriter(f, fieldnames=keys)
        w.writeheader()
        w.writerows(rows)


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
             spec["series_every"], spec["threads"], spec.get("observe")) for r in runs]
    with ProcessPoolExecutor(max_workers=jobs) as pool:
        results = list(pool.map(_run_one, work))
    failed = [r for r in results if not r["ok"]]
    rows = []
    for run, result in zip(runs, results):
        if not result["ok"]:
            continue
        last = last_row(Path(result["out"]) / "series.csv")
        # Run-level measures are averages over the observation windows; series columns are the last tick.
        measures = {k: ("" if v is None else v) for k, v in result["measures"].items()}
        rows.append({"arm": run["arm"], "point": run["point"], "seed": run["seed"], "final_sha256": result["final_sha256"], **last, **measures})
    if rows:
        write_rows(root / "summary.csv", rows)
    if failed:
        raise SystemExit(f"{len(failed)} of {len(runs)} runs failed; first: {failed[0]['out']}: {failed[0]['error']}")
    return root


def last_row(series_csv):
    with open(series_csv, newline="") as f:
        rows = list(csv.DictReader(f))
    return rows[-1]
