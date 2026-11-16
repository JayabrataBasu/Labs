"""Minimal TOML writer for lab configs (the standard library can read TOML but not write it).

Supports exactly what a config contains: tables, arrays of tables, inline tables, lists, strings,
integers and booleans. Floats are rejected on purpose: the kernel has none (DR-002).
"""
import json


def _scalar(value):
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, int):
        return str(value)
    if isinstance(value, str):
        return json.dumps(value)  # JSON string escapes are valid TOML basic strings
    raise TypeError(f"cannot write {value!r} ({type(value).__name__}) to a config")


def inline(value):
    if isinstance(value, dict):
        return "{ " + ", ".join(f"{_key(k)} = {inline(v)}" for k, v in value.items()) + " }" if value else "{}"
    if isinstance(value, (list, tuple)):
        return "[" + ", ".join(inline(v) for v in value) + "]"
    return _scalar(value)


def _key(key):
    return key if key.replace("_", "").isalnum() else json.dumps(key)


def dumps(cfg):
    """Serialises a config dict as TOML: scalars, then [world], [init], then [[rules]]."""
    out = []
    for key, value in cfg.items():
        if not isinstance(value, dict) and key != "rules":
            out.append(f"{_key(key)} = {inline(value)}")
    for key, value in cfg.items():
        if isinstance(value, dict):
            out.append(f"\n[{_key(key)}]")
            out.extend(f"{_key(k)} = {inline(v)}" for k, v in value.items())
    for rule in cfg.get("rules", []):
        out.append("\n[[rules]]")
        for k, v in rule.items():
            out.append(f"{_key(k)} = {inline(v)}")
    return "\n".join(out) + "\n"
