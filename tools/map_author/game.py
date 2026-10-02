"""The game's own verdicts, through the headless experiment runner."""

from __future__ import annotations

import json
import subprocess
import tempfile
from math import ceil
from pathlib import Path

from map_editor import catalogs
from map_editor.catalogs import load_map_settings, map_layout_path, map_settings_path, setting_number
from map_editor.constants import REPO_ROOT

from .context import MapContext

COMMAND = ("cargo", "run", "--release", "--quiet", "--", "--experiment")


# Runs `actions` on the saved map from a feet position in world metres and
# returns the runner's report. Cargo builds the game when it is stale.
def run(ctx: MapContext, spawn, actions: list[dict]) -> dict:
    script = {
        "gameplay": str(catalogs.GAMEPLAY_PATH),
        "settings": str(map_settings_path(ctx.name)),
        "layout": str(map_layout_path(ctx.name)),
        "spawn": list(spawn),
        "actions": actions,
    }
    with tempfile.TemporaryDirectory() as folder:
        path = Path(folder) / "experiment.json"
        path.write_text(json.dumps(script), encoding="utf-8")
        result = subprocess.run([*COMMAND, str(path)], cwd=REPO_ROOT, text=True, capture_output=True)
    if result.returncode:
        raise ValueError(f"the game refused the experiment: {result.stderr.strip()[-2000:]}")
    return json.loads(result.stdout)


def _setting(ctx: MapContext, path: str) -> float:
    return setting_number(load_map_settings(ctx.name), str(map_settings_path(ctx.name)), path)


def eye_height(ctx: MapContext) -> float:
    return _setting(ctx, "player.eye_height")


# The ticks to let pass between two shots.
def cooldown_ticks(ctx: MapContext) -> int:
    return ceil(_setting(ctx, "weapons.projectiles.cooldown_secs") * _setting(ctx, "network.server_hz")) + 1
