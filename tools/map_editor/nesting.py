"""Nested map placements: their motion, shape, labels, and reference checks."""

from __future__ import annotations

from dataclasses import dataclass

from .constants import NESTED_MAPS_LIST

Nudge = tuple[float, float, float]


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

    @classmethod
    def from_entry(cls, entry: dict) -> "NestedMotion":
        return cls(
            entry["map"],
            entry["to_level"],
            entry["travel_secs"],
            entry["pause_secs"],
            entry["phase_secs"],
            tuple(entry["from_nudge"]),
            tuple(entry["to_nudge"]),
        )

    def to_entry(self) -> dict:
        return {
            "map": self.map_name,
            "to_level": self.to_level,
            "travel_secs": self.travel_secs,
            "pause_secs": self.pause_secs,
            "phase_secs": self.phase_secs,
            "from_nudge": list(self.from_nudge),
            "to_nudge": list(self.to_nudge),
        }


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


def nested_map_cycle(edited: str | None, entries: list[dict], lookup) -> list[str] | None:
    """The chain of names along which a map nests itself, starting from the
    edited map's entries, or None. `lookup(name)` gives a map's shape (None
    for unknown geometry, which ends that branch)."""
    checked: set[str] = set()

    def visit(name: str, chain: list[str]) -> list[str] | None:
        if name in chain:
            return chain[chain.index(name) :] + [name]
        if name in checked:
            return None
        shape = lookup(name)
        if shape is None:
            return None
        for child in shape.nested_names:
            found = visit(child, chain + [name])
            if found:
                return found
        checked.add(name)
        return None

    root = edited or "(this map)"
    for entry in entries:
        found = visit(entry["map"], [root])
        if found:
            return found
    return None


def nested_map_rest_points(entry: dict, wall_width_cells: float) -> tuple[tuple[float, float], tuple[float, float]]:
    """Where the nested map's cell (0, 0) rests at each end, in cell units:
    the anchor plus the nudge's x and z parts. The y part is left out; the
    canvas shows the plan, and a storey's height is not a canvas distance."""

    def rest(anchor: list[int], nudge: list[float]) -> tuple[float, float]:
        if len(nudge) != 3:
            return tuple(anchor)
        return (anchor[0] + nudge[0] * wall_width_cells, anchor[1] + nudge[2] * wall_width_cells)

    return rest(entry["from"], entry["from_nudge"]), rest(entry["to"], entry["to_nudge"])


def nested_map_label(name: str, nudge: list[float]) -> str:
    """The footprint's label: the map's name, and its y nudge when it has
    one, since the plan cannot draw a vertical displacement."""
    return name if len(nudge) != 3 or nudge[1] == 0 else f"{name} y{nudge[1]:+g}"


def nested_map_error(map_name: str, edited: str | None) -> str | None:
    if not map_name:
        return "pick a map to nest"
    if edited is not None and map_name == edited:
        return "a map cannot nest itself"
    return None
