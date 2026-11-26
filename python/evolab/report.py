"""`python -m evolab html <results>`: one self-contained page describing an experiment's runs.

It shows distributions over seeds (every run is a dot), the time course of population per cell, and
the provenance of each run. It draws and tabulates; it performs no inference. Everything taken from
files is escaped.
"""
import csv
import html
import json
import math
import re
import tomllib
from pathlib import Path

from . import analysis, registered

# Charted after the primary outcome, when present and numeric.
USUAL_METRICS = ["entities", "dependent_fraction", "dependent_fraction_leaked", "lacking_leaked", "complementary", "reciprocal_excess", "reciprocal_fraction",
                 "cross_supply_share", "hosted_fraction", "host_fitness_between_excess", "synergy", "dependence"]
NOT_METRICS = {"arm", "point", "seed", "final_sha256"}
MAX_TIME_CELLS = 24

STYLE = """
:root { color-scheme: light; --page:#f9f9f9; --surface:#fff; --ink:#191919; --ink2:#666; --grid:#e7e7e7; --axis:#c2c2c2; --mark:#257de0; }
@media (prefers-color-scheme: dark) { :root { color-scheme: dark; --page:#0b0b0b; --surface:#191919; --ink:#efefef; --ink2:#9c9c9c; --grid:#303030; --axis:#474747; --mark:#338ced; } }
body { font-family: system-ui, -apple-system, "Segoe UI", sans-serif; background: var(--page); color: var(--ink); margin: 0 auto; padding: 1.2rem; max-width: 1000px; }
section { background: var(--surface); border: 1px solid var(--grid); border-radius: 6px; padding: .8rem 1rem; margin-bottom: 1rem; }
h1 { font-size: 1.3rem; } h2 { font-size: 1rem; margin: 0 0 .5rem; } h3 { font-size: .8rem; font-weight: 500; color: var(--ink2); margin: 0; }
.badge { font-size: .75rem; border: 1px solid var(--axis); border-radius: 4px; padding: .1rem .4rem; margin-left: .5rem; }
.note { color: var(--ink2); font-size: .85rem; white-space: pre-wrap; }
svg { width: 100%; height: auto; display: block; } svg text { fill: var(--ink2); font-size: 11px; font-variant-numeric: tabular-nums; }
.grid { stroke: var(--grid); stroke-width: 1; } .axis { stroke: var(--axis); stroke-width: 1; }
.dot { fill: var(--mark); stroke: var(--surface); stroke-width: 1.5; } .dot:hover { stroke: var(--ink); }
.mean { stroke: var(--ink); stroke-width: 2; } .line { fill: none; stroke: var(--mark); stroke-width: 2; } .band { fill: var(--mark); opacity: .15; }
.multiples { display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); gap: .8rem; }
table { border-collapse: collapse; font-size: .8rem; font-variant-numeric: tabular-nums; width: 100%; }
td, th { border-bottom: 1px solid var(--grid); padding: .2rem .5rem; text-align: right; } td:first-child, th:first-child, td.t, th.t { text-align: left; }
summary { cursor: pointer; font-size: .85rem; color: var(--ink2); }
"""


def esc(value):
    return html.escape(str(value), quote=True)


def fmt(v):
    if v == int(v) and abs(v) < 1e7:
        return str(int(v))
    return f"{v:.4g}"


def _nice_up(x):
    unit = 10.0 ** math.floor(math.log10(x))
    return next(k * unit for k in (1, 2, 2.5, 5, 10) if k * unit >= x * (1 - 1e-12))


def axis(values):
    """(low, high): includes zero and ends on round numbers."""
    low, high = min(values), max(values)
    low = -_nice_up(-low) if low < 0 else 0.0
    high = _nice_up(high) if high > 0 else 0.0
    return (low, high) if high > low else (low, low + 1.0)


def natural(cell):
    """Sort key for (arm, point) in which numbers compare as numbers."""
    return [[(int(part), "") if part.isdigit() else (-1, part) for part in re.split(r"(\d+)", text) if part] for text in cell]


def numeric_columns(rows):
    found = []
    for key in rows[0]:
        if key in NOT_METRICS:
            continue
        values = [r[key] for r in rows if r[key] != ""]
        try:
            ok = bool(values) and all(math.isfinite(float(v)) for v in values)
        except ValueError:
            ok = False
        if ok:
            found.append(key)
    return found


def _cell(arm, point):
    return arm if point == "base" else f"{arm} · {point}"


def dot_plot(rows, metric):
    groups = {}
    for r in rows:
        if r[metric] != "":
            groups.setdefault((r["arm"], r["point"]), []).append((r["seed"], float(r[metric])))
    low, high = axis([v for g in groups.values() for _, v in g])
    longest = max(len(_cell(*k)) for k in groups)
    left, width, step = min(420, 12 + 6.5 * longest), 960, 22
    top = 16 if len(groups) > 12 else 0  # tall plots are labelled at both ends
    height = step * len(groups) + 24 + top

    def x(v):
        return left + (v - low) / (high - low) * (width - left - 20)

    parts = [f'<svg viewBox="0 0 {width} {height}" role="img" aria-label="{esc(metric)} per cell, one dot per run">']
    for k in range(5):
        v = low + (high - low) * k / 4
        parts.append(f'<line class="{"axis" if v == 0 else "grid"}" x1="{x(v):.2f}" x2="{x(v):.2f}" y1="{top}" y2="{height - 18}"/>'
                     f'<text x="{x(v):.2f}" y="{height - 5}" text-anchor="middle">{fmt(v)}</text>')
        if top:
            parts.append(f'<text x="{x(v):.2f}" y="11" text-anchor="middle">{fmt(v)}</text>')
    for row, key in enumerate(sorted(groups, key=natural)):
        y = top + step * row + step / 2
        values = groups[key]
        mean = sum(v for _, v in values) / len(values)
        parts.append(f'<text x="{left - 8}" y="{y + 4}" text-anchor="end">{esc(_cell(*key))}</text>')
        parts.append(f'<line class="mean" x1="{x(mean):.2f}" x2="{x(mean):.2f}" y1="{y - 8}" y2="{y + 8}"><title>mean {fmt(mean)}</title></line>')
        for seed, v in values:
            parts.append(f'<circle class="dot" cx="{x(v):.2f}" cy="{y}" r="4.5"><title>{esc(_cell(*key))}, seed {esc(seed)}: {fmt(v)}</title></circle>')
    parts.append("</svg>")
    return "".join(parts)


def stats_table(rows, metric):
    head = "<tr><th>arm</th><th class=t>point</th><th>n</th><th>mean</th><th>sd</th><th>min</th><th>median</th><th>max</th></tr>"
    body = "".join(
        f"<tr><td>{esc(r['arm'])}</td><td class=t>{esc(r['point'])}</td><td>{r['n']}</td>"
        + "".join(f"<td>{fmt(r[k])}</td>" for k in ("mean", "sd", "min", "median", "max")) + "</tr>"
        for r in sorted(analysis.table(rows, metric), key=lambda r: natural((r["arm"], r["point"]))))
    return f"<details><summary>Table</summary><table>{head}{body}</table></details>"


def series_summary(runs):
    """[(tick, min, mean, max)] over runs, each a list of (tick, value), on the ticks all runs share."""
    if not runs:
        return []
    lookup = [dict(r) for r in runs]
    ticks = [t for t, _ in runs[0] if all(t in d for d in lookup)]
    out = []
    for t in ticks:
        values = [d[t] for d in lookup]
        out.append((t, min(values), sum(values) / len(values), max(values)))
    return out


def time_plot(points, high, label):
    width, height, left = 320, 120, 40
    first, last = points[0][0], points[-1][0]

    def x(t):
        return left + (0 if last == first else (t - first) / (last - first)) * (width - left - 6)

    def y(v):
        return 6 + (1 - v / high) * (height - 24)

    band = " ".join(f"{x(t):.1f},{y(hi):.1f}" for t, _, _, hi in points) + " " + " ".join(f"{x(t):.1f},{y(lo):.1f}" for t, lo, _, _ in reversed(points))
    line = " ".join(f"{x(t):.1f},{y(m):.1f}" for t, _, m, _ in points)
    grid = "".join(f'<line class="{"axis" if v == 0 else "grid"}" x1="{left}" x2="{width - 6}" y1="{y(v):.1f}" y2="{y(v):.1f}"/>'
                   f'<text x="{left - 4}" y="{y(v) + 4:.1f}" text-anchor="end">{fmt(v)}</text>' for v in (0, high / 2, high))
    return (f'<svg viewBox="0 0 {width} {height}" role="img" aria-label="{esc(label)}">{grid}<polygon class="band" points="{band}"/>'
            f'<polyline class="line" points="{line}"><title>{esc(label)}: mean over seeds; band from lowest to highest run</title></polyline>'
            f'<text x="{left}" y="{height - 3}">tick {first}</text><text x="{width - 6}" y="{height - 3}" text-anchor="end">{last}</text></svg>')


def _run_dir(root, row):
    return root / row["arm"] / row["point"] / f"seed-{row['seed']}"


def time_section(root, rows, column="entities"):
    cells = {}
    for r in rows:
        path = _run_dir(root, r) / "series.csv"
        if path.is_file():
            with open(path, newline="") as f:
                cells.setdefault((r["arm"], r["point"]), []).append([(int(s["tick"]), float(s[column])) for s in csv.DictReader(f)])
    summaries = {k: series_summary(cells[k]) for k in sorted(cells, key=natural)}
    summaries = {k: v for k, v in summaries.items() if v}
    if not summaries:
        return ""
    high = axis([p[3] for v in summaries.values() for p in v])[1]
    shown = list(summaries.items())[:MAX_TIME_CELLS]
    charts = "".join(f"<div><h3>{esc(_cell(*k))}</h3>{time_plot(v, high, _cell(*k))}</div>" for k, v in shown)
    more = f'<p class="note">Showing the first {MAX_TIME_CELLS} of {len(summaries)} cells.</p>' if len(summaries) > MAX_TIME_CELLS else ""
    return (f'<section><h2>{esc(column.capitalize())} over time</h2><p class="note">Line: mean over seeds. Band: lowest to highest run. '
            f'All panels share one scale.</p><div class="multiples">{charts}</div>{more}</section>')


def provenance(root, rows):
    body = []
    for r in rows:
        manifest = _run_dir(root, r) / "manifest.json"
        info = json.loads(manifest.read_text()) if manifest.is_file() else {}
        body.append(f"<tr><td>{esc(r['arm'])}</td><td class=t>{esc(r['point'])}</td><td>{esc(r['seed'])}</td>"
                    f"<td class=t><code>{esc(r['final_sha256'][:16])}</code></td><td class=t><code>{esc(str(info.get('git_commit', ''))[:10])}</code></td>"
                    f"<td class=t>{esc(info.get('git_dirty', ''))}</td></tr>")
    return ("<section><h2>Runs</h2><details><summary>Provenance of every run</summary><table><tr><th>arm</th><th class=t>point</th><th>seed</th>"
            "<th class=t>final state hash</th><th class=t>code</th><th class=t>uncommitted changes</th></tr>" + "".join(body) + "</table></details></section>")


def fmt_p(p):
    """Three significant digits, so that a small p-value never reads as zero."""
    return f"{p:.3g}"


def tests_section(root):
    results = registered.evaluate(root)
    if not results:
        return ""
    def cell(key, v):
        if v is None:
            return ""
        return fmt_p(v) if key.startswith("p") and key != "point" else fmt(v) if isinstance(v, float) else esc(v)
    columns = [("name", "hypothesis"), ("point", "cell"), ("metric", "metric"), ("arm", "arm"), ("against", "against"), ("n", "n"),
               ("n_against", "n against"), ("missing", "missing"), ("estimate", "estimate"), ("low", "95% low"), ("high", "95% high"),
               ("smallest_effect", "smallest effect"), ("p", "p"), ("p_adjusted", "p (Holm)"), ("verdict", "verdict")]
    head = "".join(f"<th>{label}</th>" for _, label in columns)
    body = "".join("<tr>" + "".join(f"<td>{cell(key, r[key])}</td>" for key, _ in columns) + "</tr>" for r in results)
    return (f'<section><h2>Registered tests</h2><div style="overflow-x:auto"><table><tr>{head}</tr>{body}</table></div>'
            '<p class="note">Declared in the specification. Two-sided randomization tests over runs; percentile bootstrap '
            'intervals; Holm adjustment across this table.</p></section>')


def build(root):
    root = Path(root)
    meta = json.loads((root / "experiment.json").read_text())
    rows = analysis.load_summary(root)
    try:
        spec = tomllib.loads(meta.get("spec_text", ""))
    except tomllib.TOMLDecodeError:
        spec = {}
    numeric = numeric_columns(rows)
    primary = meta.get("primary_outcome", "")
    chosen = [m for m in dict.fromkeys([primary, *USUAL_METRICS]) if m in numeric]
    status = ("Registered", "Run from a frozen specification.") if meta.get("registered") else \
             ("Exploratory", "Not a registered design: use these results to form questions, not to answer them.")
    about = "".join(f"<p class=note><strong>{label}:</strong> {esc(value)}</p>" for label, value in (
        ("Aim", spec.get("aim", "")), ("Primary outcome", primary), ("Estimand", spec.get("estimand", "")),
        ("Analysis plan", meta.get("analysis_plan", ""))) if value)
    sections = "".join(
        f'<section data-metric="{esc(m)}"><h2>{esc(m)}{" (primary outcome)" if m == primary else ""}</h2>'
        f"{dot_plot(rows, m)}{stats_table(rows, m)}</section>" for m in chosen)
    return (
        f'<!DOCTYPE html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width">'
        f'<link rel="icon" href="data:,"><title>{esc(meta["name"])} · report</title><style>{STYLE}</style></head><body>'
        f'<h1>{esc(meta["name"])}<span class="badge">{status[0]}</span></h1>'
        f'<p class="note">{status[1]} {len(rows)} runs · {esc(meta.get("lab_version", ""))} · spec sha256 {esc(meta.get("spec_sha256", "")[:16])}</p>'
        f'<section>{about}<p class="note">A result here means only that the stated rules and random process produced it. '
        f'It is not a claim about real organisms or organizations.</p></section>'
        f'{tests_section(root)}{sections}<p class="note">Each dot is one run; the vertical stroke is the mean over seeds. Run-level measures average the '
        f'observation windows; counts such as entities are taken at the last tick.</p>'
        f'{time_section(root, rows)}{provenance(root, rows)}'
        f'<details><summary>Specification</summary><pre>{esc(meta.get("spec_text", ""))}</pre></details></body></html>')


def write(root):
    path = Path(root) / "report.html"
    path.write_text(build(root), encoding="utf-8")
    return path
