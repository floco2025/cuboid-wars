"""Native map-core adapter. The Rust library owns source and geometry rules."""

from __future__ import annotations

import importlib.util
import json
import math
import os
import subprocess
import sys
from dataclasses import asdict, is_dataclass
from pathlib import Path

_ROOT = Path(__file__).resolve().parents[2]
_native = None


def _load():
    global _native
    if _native is not None:
        return _native
    # Ask Cargo for the artifact path: this also respects custom target directories.
    command = ["cargo", "build", "--release", "-p", "map_core_py", "--message-format=json-render-diagnostics"]
    try:
        result = subprocess.run(
            command, cwd=_ROOT, text=True, capture_output=True, env={**os.environ, "PYO3_PYTHON": sys.executable}
        )
    except OSError as error:
        raise RuntimeError(f"Cannot build the editor's Rust map library: cargo did not start ({error})") from error
    artifacts = [json.loads(line) for line in result.stdout.splitlines() if line.startswith("{")]
    if result.returncode:
        diagnostics = "".join(
            item.get("message", {}).get("rendered", "")
            for item in artifacts
            if item.get("reason") == "compiler-message"
        )
        raise RuntimeError("Cannot build the editor's Rust map library:\n" + diagnostics + result.stderr)
    paths = [
        Path(name)
        for item in artifacts
        if item.get("reason") == "compiler-artifact" and item.get("target", {}).get("name") == "_map_core"
        for name in item.get("filenames", [])
        if name.endswith((".so", ".dylib", ".dll", ".pyd"))
    ]
    if not paths:
        raise RuntimeError("Cargo did not produce the editor's Rust map library")
    # ExtensionFileLoader accepts Cargo's native filename on each platform.
    from importlib.machinery import ExtensionFileLoader

    spec = importlib.util.spec_from_file_location(
        "_map_core", paths[-1], loader=ExtensionFileLoader("_map_core", str(paths[-1]))
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    _native = module
    return module


def _encode(value):
    # JSON cannot carry non-finite floats. Keep malformed authored values intact
    # across the binding so the native validator can diagnose them.
    if isinstance(value, float) and not math.isfinite(value):
        return {"$map_core_float": str(value)}
    if is_dataclass(value):
        value = asdict(value)
    if isinstance(value, dict):
        if "$map_core_float" in value or "$map_core_object" in value:
            return {"$map_core_object": [[key, _encode(item)] for key, item in value.items()]}
        return {key: _encode(item) for key, item in value.items()}
    if isinstance(value, (list, tuple, set)):
        return [_encode(item) for item in value]
    return value


def _decode(value):
    if isinstance(value, dict):
        if value.keys() == {"$map_core_object"}:
            return {key: _decode(item) for key, item in value["$map_core_object"]}
        if value.keys() == {"$map_core_float"}:
            return float(value["$map_core_float"])
        return {key: _decode(item) for key, item in value.items()}
    if isinstance(value, list):
        return [_decode(item) for item in value]
    return value


def call(operation, *args):
    return _decode(json.loads(_load().call(operation, json.dumps(_encode(args), allow_nan=False))))


# The canvas asks for a record's cells and levels once per record on every mouse
# move, which no boundary crossing is cheap enough for; those few helpers read
# coordinates in Python the way map_core's `int` does, and
# tests/test_core_parity.py holds them to the Rust results.
def grid_int(value) -> int:
    if type(value) is int:
        return value
    return int(value) if type(value) is float and math.isfinite(value) else 0


def grid_point(value) -> tuple[int, int]:
    pair = value if isinstance(value, (list, tuple)) else ()
    return tuple(grid_int(pair[index]) if index < len(pair) else 0 for index in (0, 1))


def tuples(value):
    """Restore hashable keys and coordinates at the Python boundary."""
    return tuple(tuples(item) for item in value) if isinstance(value, list) else value


def shapes(entries, lookup):
    """Materialize a Python lookup callback; cycle detection stays in Rust."""
    if lookup is None:
        return None
    found = {}
    pending = [entry.get("map") for entry in entries]
    while pending:
        name = pending.pop()
        if name in found:
            continue
        shape = lookup(name)
        found[name] = shape
        if shape is not None:
            pending.extend(shape.nested_names)
    return found
