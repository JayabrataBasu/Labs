"""Reading what the `lab` binary writes: entity tables and observation windows."""
import csv
import json
from pathlib import Path
from typing import NamedTuple


class Gene(NamedTuple):
    kind: str  # "uptake", "convert", "secrete" or "bind"
    a: int  # resource index (source, for convert)
    b: int  # resource index (product, for convert)
    sense: int | None  # gating resource, None = always on
    threshold: int
    level: int


class Flow(NamedTuple):
    entity: int
    site: int
    resource: int
    taken: int
    released: int
    produced: int


class Entity(NamedTuple):
    id: int
    site: int
    mother: int | None
    born: int
    stocks: tuple
    genome: tuple
    host: int | None = None  # id of the entity that contains this one


class Window(NamedTuple):
    start: int
    end: int
    width: int
    height: int
    wrap: bool
    resources: list
    entities_start: dict
    entities_end: dict
    flows: list
    births: list  # (tick, child, mother)
    deaths: list  # (tick, id)


def parse_genome(text):
    """Genome syntax of the entity table: `kind:a>b@sense>=threshold*level` joined by `;`."""
    genes = []
    for part in filter(None, text.split(";")):
        kind, rest = part.split(":", 1)
        resources, rest = rest.split("@", 1)
        gate, level = rest.rsplit("*", 1)
        a, b = resources.split(">")
        sense, threshold = (None, 0) if gate == "always" else (int(x) for x in gate.split(">="))
        genes.append(Gene(kind, int(a), int(b), sense, threshold, int(level)))
    return genes


def read_entities(path):
    cache = {}
    out = {}
    with open(path, newline="") as f:
        for row in csv.DictReader(f):
            text = row["genome"]
            if text not in cache:
                cache[text] = tuple(parse_genome(text))
            stocks = tuple(int(row[k]) for k in row if k.startswith("stock_"))
            mother = int(row["mother"]) if row["mother"] else None
            host = int(row["host"]) if row.get("host") else None
            out[int(row["id"])] = Entity(int(row["id"]), int(row["site"]), mother, int(row["born"]), stocks, cache[text], host)
    return out


def _ints(path):
    with open(path) as f:
        next(f)
        return [tuple(map(int, line.split(","))) for line in f]


def read_window(directory):
    d = Path(directory)
    meta = json.loads((d / "meta.json").read_text())
    return Window(meta["start"], meta["end"], meta["width"], meta["height"], meta["wrap"], meta["resources"],
                  read_entities(d / "entities-start.csv"), read_entities(d / "entities-end.csv"),
                  [Flow(*row) for row in _ints(d / "flows.csv")], _ints(d / "births.csv"), _ints(d / "deaths.csv"))


def windows(run_dir):
    """The observation windows of a run, oldest first."""
    return sorted((Path(run_dir) / "obs").glob("window-*"))
