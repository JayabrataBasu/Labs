"""python -m evolab freeze|run|report|html|check|calibrate ... (set PYTHONPATH=python)"""
import argparse
import sys
from pathlib import Path

from . import analysis, calibrate, report, runner, spec


def main(argv=None):
    ap = argparse.ArgumentParser(prog="evolab")
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("freeze", help="record the spec and every run config; later changes are detected")
    p.add_argument("spec")
    p = sub.add_parser("run", help="run all arms x sweep points x seeds")
    p.add_argument("spec")
    p.add_argument("--out", required=True)
    p.add_argument("--jobs", type=int, required=True, help="runs in parallel")
    p.add_argument("--exploratory", action="store_true", help="allow an unfrozen spec; results are marked unregistered")
    p = sub.add_parser("report", help="distributions of an outcome over seeds, per arm and sweep point")
    p.add_argument("results")
    p.add_argument("--metric", required=True)
    p = sub.add_parser("html", help="write a self-contained report.html with charts into the results folder")
    p.add_argument("results")
    p = sub.add_parser("check", help="construct check of an outcome")
    p.add_argument("results")
    p.add_argument("--metric", required=True)
    p = sub.add_parser("calibrate", help="birth and death probabilities for the neutral null that match a finished run")
    p.add_argument("run", help="folder of one intact run (manifest.json and series.csv)")
    p.add_argument("--from-tick", type=int, required=True, help="measure demography from this tick on")
    p.add_argument("--ticks", type=int, required=True, help="length of each trial run")
    p.add_argument("--work", required=True, help="folder for the trial runs")
    a = ap.parse_args(argv)
    if a.cmd == "freeze":
        r = spec.freeze(a.spec)
        print(f"frozen {r['n_runs']} runs; spec sha256 {r['spec_sha256']}")
    elif a.cmd == "run":
        root = runner.run_experiment(a.spec, a.out, a.jobs, a.exploratory)
        print(f"results in {root}")
    elif a.cmd == "report":
        rows = analysis.load_summary(a.results)
        print(f"{'arm':<22}{'point':<28}{'n':>3}" + "".join(f"{k:>11}" for k in ("mean", "sd", "min", "median", "max")))
        for r in analysis.table(rows, a.metric):
            numbers = "".join(f"{report.fmt(r[k]):>11}" for k in ("mean", "sd", "min", "median", "max"))
            print(f"{r['arm']:<22}{r['point']:<28}{r['n']:>3}{numbers}")
    elif a.cmd == "html":
        print(f"wrote {report.write(a.results)}")
    elif a.cmd == "calibrate":
        for key, value in calibrate.run(runner.lab_binary(), a.run, a.from_tick, a.ticks, a.work).items():
            print(f"{key} = {value:.0f}")
    elif a.cmd == "check":
        ok_all = True
        for ok, msg in analysis.construct_check(analysis.load_summary(a.results), a.metric):
            print(("PASS  " if ok else "FAIL  ") + msg)
            ok_all &= ok
        return 0 if ok_all else 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
