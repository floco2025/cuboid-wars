"""Editor color and label presentation helpers."""

from __future__ import annotations

import hashlib

from PySide6.QtGui import QColor

from .constants import (
    DEFAULT_ALIAS,
    FACES,
    FIREWORK_PLATE_COLOR,
    MODE_BRIDGE_PLATE,
    MODE_ERASE,
    MODE_ERASE_GRASS,
    MODE_ERASE_ITEMS,
    MODE_ERASE_KEEP_FLOORS,
    MODE_ERASE_LADDERS,
    MODE_ERASE_LIGHT_BRIDGES,
    MODE_ERASE_LIGHTS,
    MODE_ERASE_PRESSURE_PLATES,
    MODE_FIREWORK_PLATE,
    MODE_FLOOR,
    MODE_FLOOR_MATERIAL,
    MODE_GRASS,
    MODE_INACCESSIBLE_FLOOR,
    MODE_ITEM,
    MODE_LADDER,
    MODE_LIGHT_BRIDGE,
    MODE_PLAYER_SPAWN_PAINT,
    MODE_PRESSURE_PLATE,
    MODE_RAMP_MATERIAL,
)

def zone_color(kind: str) -> QColor:
    if not kind:
        return QColor(34, 197, 94)
    return tag_color(kind)


def tag_color(tag: str) -> QColor:
    digest = hashlib.md5(tag.encode("utf-8")).digest()
    hue = (digest[0] | (digest[1] << 8)) % 360
    color = QColor()
    color.setHsv(hue, 165, 220)
    return color


WALL_PEN_WIDTH = 6
WALL_HIGHLIGHT_WIDTH = WALL_PEN_WIDTH + 4
# Barriers render slightly thinner than walls so the two read as distinct
# even when their colors happen to be close.
BARRIER_PEN_WIDTH = 4

# Translucent rectangle preview drawn while dragging in modes that operate on
# a cell rectangle. Lookup falls back to a neutral green for any mode that
# uses the rect-preview UI but isn't listed here (e.g. actor spawn paint).
DRAG_PREVIEW_FALLBACK = QColor(34, 197, 94, 120)


def _translucent(hex_color: str, alpha: int) -> QColor:
    color = QColor(hex_color)
    color.setAlpha(alpha)
    return color


DRAG_PREVIEW_COLORS: dict[str, QColor] = {
    MODE_FLOOR: QColor(111, 180, 255, 120),
    MODE_INACCESSIBLE_FLOOR: QColor(148, 163, 184, 120),
    MODE_GRASS: QColor(132, 204, 22, 120),  # lime — matches the grass tuft strokes
    MODE_ERASE_GRASS: QColor(120, 113, 108, 120),  # stone — mowed-down grass, not a red erase tool
    MODE_PLAYER_SPAWN_PAINT: QColor(99, 102, 241, 120),
    # Type is picked *after* the click, so the hover ghost is a neutral
    # off-white. The placed glyph is then color-coded by its type.
    MODE_ITEM: QColor(220, 220, 220, 110),
    MODE_ERASE_ITEMS: QColor(245, 158, 11, 120),  # amber family, like Erase Lights
    MODE_LADDER: QColor(251, 146, 60, 120),  # orange — matches the ladder glyph
    MODE_ERASE_LADDERS: QColor(251, 146, 60, 120),
    # Kind is picked after the drag, so the preview uses the generic bridge cyan.
    MODE_LIGHT_BRIDGE: QColor(48, 216, 255, 120),
    MODE_ERASE_LIGHT_BRIDGES: QColor(245, 158, 11, 120),  # amber family, like Erase Items
    MODE_FLOOR_MATERIAL: QColor(236, 72, 153, 120),
    MODE_RAMP_MATERIAL: QColor(168, 85, 247, 120),  # purple to distinguish from floor mode pink
    MODE_ERASE: QColor(248, 113, 113, 120),
    MODE_ERASE_KEEP_FLOORS: QColor(251, 146, 60, 120),
    MODE_ERASE_LIGHTS: QColor(250, 204, 21, 120),   # amber — distinct from red erase tools
    # Kind is picked after the click (like items): neutral off-white ghost.
    MODE_PRESSURE_PLATE: QColor(220, 220, 220, 110),
    MODE_BRIDGE_PLATE: QColor(220, 220, 220, 110),
    MODE_FIREWORK_PLATE: _translucent(FIREWORK_PLATE_COLOR, 120),
    MODE_ERASE_PRESSURE_PLATES: QColor(245, 158, 11, 120),  # amber family, like Erase Items
}


def face_color(seg: dict) -> QColor:
    """Color derived from the segment's full six-face material composition.
    Two segments (floor / wall / ramp) sharing the same six face values get
    the same color; differing on any face produces a different one. Saturation
    is pinned to max so hue differences read clearly; value also varies a bit
    so close hues remain distinguishable."""
    digest = hashlib.md5("|".join(seg.get(face, "") for face in FACES).encode("utf-8")).digest()
    hue = int.from_bytes(digest[:2], "big") % 360
    value = 200 + (digest[2] % 56)  # 200-255
    color = QColor()
    color.setHsv(hue, 255, value)
    return color


def expand_face_materials(obj: dict) -> dict[str, str]:
    """Expand `all` shorthand into six explicit face materials. Faces not
    explicitly set fall back to `all` (or to any other face value if `all` is
    absent). Used when reading per-segment material data from JSON."""
    fallback = obj.get("all")
    if fallback is None:
        fallback = next((obj[face] for face in FACES if face in obj), None)
    if fallback is None:
        # Segment loaded without any material data — fall back to a *legal*
        # value pulled from the loaded alias catalog (face values are
        # validated against aliases on save).
        fallback = DEFAULT_ALIAS
    return {face: obj.get(face, fallback) for face in FACES}


def compact_face_materials(faces: dict[str, str]) -> dict:
    """Pack six face materials into the on-disk `all` + overrides shape.
    Picks the most-common face value as `all`; ties broken alphabetically for
    deterministic output."""
    counts: dict[str, int] = {}
    for face in FACES:
        if face in faces:
            counts[faces[face]] = counts.get(faces[face], 0) + 1
    if not counts:
        return {}
    best_count = max(counts.values())
    most_common = sorted(name for name, count in counts.items() if count == best_count)[0]
    if best_count <= 1:
        return {face: faces[face] for face in FACES if face in faces}
    out = {"all": most_common}
    for face in FACES:
        if face in faces and faces[face] != most_common:
            out[face] = faces[face]
    return out


def materials_summary(seg: dict) -> str:
    """One-line summary of a segment's six face materials, using the same
    `all`/overrides compaction as the on-disk shape."""
    compact = compact_face_materials(seg)
    return ", ".join(f"{k}={v}" for k, v in compact.items())


def level_label(level: dict, index: int) -> str:
    name = level.get("name")
    return f"Level {index}" if not name else f"Level {index} ({name})"


