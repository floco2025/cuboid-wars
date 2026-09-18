"""Nested map placements: their motion, shape, labels, and reference checks."""

from __future__ import annotations

from collections.abc import Sequence
from dataclasses import dataclass

from .constants import NESTED_MAPS_LIST
from .core import call, shapes, tuples

Nudge = tuple[float, float, float]
Footprint = tuple[float, float, float, float]
DEFAULT_MOTION = "cycle"
MOTION_LABELS = {DEFAULT_MOTION: "Cycle", "follow_switch": "Follow switch"}
MOTION_TOOLTIPS = {
    "travel_secs": "Time to travel the full distance between the two ends.",
    "pause_secs": "Pause at each end of the cycle.",
    "phase_secs": "Start this far into the cycle.",
}


def motion_uses_cycle(motion) -> bool:
    """Whether the cycle pause and phase apply: only Cycle repeats, while
    Follow switch holds at whichever end its switch sends it to."""
    return motion == DEFAULT_MOTION


def nested_map_starts_at_end_2(entry: dict):
    return call("nested_map_starts_at_end_2", entry)


@dataclass(frozen=True)
class NestedMotion:
    """How a placed nested map travels: everything in its entry except where
    it starts. The last answer is remembered for the next placement."""

    map_name: str
    to_level: int
    travel_secs: float
    pause_secs: float
    phase_secs: float
    from_nudge: Nudge
    to_nudge: Nudge
    # Follow switch requires this; a Cycle without it runs continuously.
    switch: str | None = None
    switch_inverted: bool = False
    motion: str = DEFAULT_MOTION

    @classmethod
    def from_entry(cls, entry: dict) -> "NestedMotion":
        # An unknown motion stays on the record for validation; it is no
        # placement default.
        motion = entry.get("motion")
        return cls(
            entry["map"],
            entry["to_level"],
            entry["travel_secs"],
            entry["pause_secs"],
            entry["phase_secs"],
            tuple(entry["from_nudge"]),
            tuple(entry["to_nudge"]),
            entry.get("switch") or None,
            entry.get("switch_inverted", False),
            motion if motion in MOTION_LABELS else DEFAULT_MOTION,
        )

    def to_entry(self) -> dict:
        entry = {
            "map": self.map_name,
            "motion": self.motion,
            "to_level": self.to_level,
            "travel_secs": self.travel_secs,
            "pause_secs": self.pause_secs,
            "phase_secs": self.phase_secs,
            "from_nudge": list(self.from_nudge),
            "to_nudge": list(self.to_nudge),
        }
        if self.switch:
            entry["switch"] = self.switch
            entry["switch_inverted"] = self.switch_inverted
        return entry


@dataclass(frozen=True)
class NestedMapShape:
    """The footprint, storeys, and references of named nested geometry."""

    grid_cols: int
    grid_rows: int
    level_count: int
    nested_names: tuple[str, ...]


def nested_map_shape(data: dict | None) -> NestedMapShape | None:
    if data is None:
        return None
    return NestedMapShape(
        grid_cols=data["grid_cols"],
        grid_rows=data["grid_rows"],
        level_count=len(data["levels"]),
        nested_names=tuple(entry["map"] for entry in data.get(NESTED_MAPS_LIST, [])),
    )


def nested_map_cycle(edited: str | None, entries: list[dict], lookup):
    return call("nested_map_cycle", edited, entries, shapes(entries, lookup))


def nested_map_rest_points(entry: dict, wall_width_cells: float):
    return tuples(call("nested_map_rest_points", entry, wall_width_cells))


def nested_map_footprint(anchor: Sequence[float], shape: NestedMapShape | None) -> Footprint:
    """The cells the nested map's grid covers with its cell (0, 0) on
    `anchor`, as (x0, y0, x1, y1); one cell for unknown geometry, so the
    canvas has something to outline."""
    cols, rows = (shape.grid_cols, shape.grid_rows) if shape else (1, 1)
    return (anchor[0], anchor[1], anchor[0] + cols, anchor[1] + rows)


def nested_map_footprints(entry: dict, shape: NestedMapShape | None, wall_width_cells: float):
    return tuples(call("nested_map_footprints", entry, shape, wall_width_cells))


def nested_map_label(name: str | None, nudge: Sequence[float] = (), *, known: bool = True) -> str:
    """The footprint's label: the map's name, and its y nudge when it has
    one, since the plan cannot draw a vertical displacement; a question
    mark when its geometry is unknown, asking to be fixed."""
    if not known:
        return f"{name or '?'}?"
    return name if len(nudge) != 3 or nudge[1] == 0 else f"{name} y{nudge[1]:+g}"


def nested_map_error(map_name: str, edited: str | None) -> str | None:
    if not map_name:
        return "pick a map to nest"
    if edited is not None and map_name == edited:
        return "a map cannot nest itself"
    return None
