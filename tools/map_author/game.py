"""The game's own verdicts, through the headless experiment runner."""

from __future__ import annotations

import json
import subprocess
import tempfile
from dataclasses import dataclass
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


# Where every attempt of a check begins: a fresh session with the body at
# the attempt's start, or a route's state through one of its steps with the
# body moved there, so a late step is tried with what the route built up.
@dataclass(frozen=True)
class Start:
    spawn: tuple[float, ...] | None = None
    prefix: tuple[dict, ...] = ()
    # How the checks name it after their standing point.
    label: str = ""

    # `<route.json>:<step>`, the step numbered as `proof` prints it.
    @classmethod
    def after(cls, spec: str) -> Start:
        path, _, step = spec.rpartition(":")
        if not path or not step.isdigit():
            raise ValueError(f"--after {spec!r} is not <route.json>:<step>")
        script = json.loads(Path(path).read_text(encoding="utf-8"))
        actions, last = script["actions"], int(step)
        if last >= len(actions):
            raise ValueError(f"{path} has steps 0 to {len(actions) - 1}, so no step {last}")
        return cls(tuple(script["spawn"]), tuple(actions[: last + 1]), f" after step {last} of {Path(path).name}")

    def spawn_for(self, feet):
        return self.spawn if self.prefix else feet

    # The actions that bring a run that has just begun to `feet`.
    def lead(self, feet) -> list[dict]:
        return [*self.prefix, {"action": "teleport", "feet": list(feet)}] if self.prefix else []

    # The actions that begin another attempt in the same run at `feet`; the
    # last one's status says whether the body could stand there.
    def begin(self, feet) -> list[dict]:
        return [{"action": "reset"}, *self.lead(feet)] if self.prefix else [{"action": "reset", "spawn": list(feet)}]


def _setting(ctx: MapContext, path: str) -> float:
    return setting_number(load_map_settings(ctx.name), str(map_settings_path(ctx.name)), path)


def eye_height(ctx: MapContext) -> float:
    return _setting(ctx, "player.eye_height")


# The ticks to let pass between two shots.
def cooldown_ticks(ctx: MapContext) -> int:
    return ceil(_setting(ctx, "weapons.projectiles.cooldown_secs") * _setting(ctx, "network.server_hz")) + 1
