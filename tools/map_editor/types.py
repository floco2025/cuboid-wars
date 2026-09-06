"""Small value types for editor state."""

from __future__ import annotations

from dataclasses import dataclass


@dataclass
class ZoneRef:
    """Identifies a spawn zone by which list it belongs to and its index."""

    list_name: str
    index: int


@dataclass
class SpawnZoneDrag:
    """In-flight spawn-zone resize/move state, driven by Alt/Option selection."""

    list_name: str
    index: int
    handle: str  # "move" or one of "n"/"s"/"e"/"w"/"nw"/"ne"/"sw"/"se"
    origin: tuple[float, float]  # cursor position when drag started, in cell coords
    original_zone: dict  # snapshot of the zone before the drag
    current: tuple[float, float] | None = None  # latest cursor position, set on each mouse-move
