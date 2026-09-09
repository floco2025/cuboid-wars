"""Editor color and label presentation helpers."""

from __future__ import annotations

import hashlib

from PySide6.QtGui import QColor

from .constants import (
    FACES,
    MODE_ERASE,
    MODE_ERASE_BARRIERS,
    MODE_ERASE_EQUIPMENT_ERASERS,
    MODE_ERASE_FLOORS,
    MODE_ERASE_GRASS,
    MODE_ERASE_ITEMS,
    MODE_ERASE_KEEP_FLOORS,
    MODE_ERASE_LADDERS,
    MODE_ERASE_LIGHT_BRIDGES,
    MODE_ERASE_LIGHTS,
    MODE_ERASE_PRESSURE_PLATES,
    MODE_ERASE_RAMPS,
    MODE_ERASE_SPAWN_ZONES,
    MODE_ERASE_WALLS,
    MODE_FLOOR,
    MODE_FLOOR_MATERIAL,
    MODE_GRASS,
    MODE_INACCESSIBLE_FLOOR,
    MODE_ITEM,
    MODE_LADDER,
    MODE_LIGHT_BRIDGE,
    MODE_NESTED_MAP,
    MODE_ERASE_NESTED_MAPS,
    MODE_PLAYER_SPAWN_ZONE,
    MODE_CHECKPOINT,
    MODE_RAMP_MATERIAL,
    PLATE_TYPE_BARRIER,
    PLATE_TYPE_BRIDGE,
    PLATE_TYPE_FIREWORK,
)
from .normalization import compact_face_materials

PLATE_LABELS = {
    PLATE_TYPE_BARRIER: ("B", "Barrier"),
    PLATE_TYPE_BRIDGE: ("L", "Light bridge"),
    PLATE_TYPE_FIREWORK: ("F", "Firework"),
}


def pressure_plate_label(plate: dict) -> str:
    _, label = PLATE_LABELS.get(plate.get("type"), ("?", "Pressure plate"))
    if plate.get("type") in PLATE_LABELS:
        label += " pressure plate"
    return f"{label}: {plate['kind']}" if "kind" in plate else label


def contrasting_text_color(color: QColor) -> QColor:
    channels = [
        channel / 12.92 if channel <= 0.04045 else ((channel + 0.055) / 1.055) ** 2.4
        for channel in (color.redF(), color.greenF(), color.blueF())
    ]
    luminance = sum(channel * weight for channel, weight in zip(channels, (0.2126, 0.7152, 0.0722)))
    return QColor("#000000" if luminance > 0.179 else "#ffffff")


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

# A nested map on the canvas: its anchors, the band between them, and its
# footprint outlined and named.
NESTED_MAP_COLOR = QColor(167, 139, 250)


DRAG_PREVIEW_COLORS: dict[str, QColor] = {
    MODE_FLOOR: QColor(111, 180, 255, 120),
    MODE_INACCESSIBLE_FLOOR: QColor(148, 163, 184, 120),
    MODE_GRASS: QColor(132, 204, 22, 120),  # lime — matches the grass tuft strokes
    MODE_ERASE_GRASS: QColor(120, 113, 108, 120),  # stone — mowed-down grass, not a red erase tool
    MODE_PLAYER_SPAWN_ZONE: QColor(99, 102, 241, 120),
    MODE_CHECKPOINT: QColor(255, 179, 31, 90),
    # Type is picked *after* the click, so the hover ghost is a neutral
    # off-white. The placed glyph is then color-coded by its type.
    MODE_ITEM: QColor(220, 220, 220, 110),
    MODE_ERASE_ITEMS: QColor(245, 158, 11, 120),  # amber family, like Erase Lights
    MODE_LADDER: QColor(251, 146, 60, 120),  # orange — matches the ladder glyph
    MODE_ERASE_LADDERS: QColor(251, 146, 60, 120),
    # Kind is picked after the drag, so the preview uses the generic bridge cyan.
    MODE_LIGHT_BRIDGE: QColor(48, 216, 255, 120),
    MODE_NESTED_MAP: QColor(167, 139, 250, 120),  # violet — matches the placed footprint
    MODE_ERASE_NESTED_MAPS: QColor(245, 158, 11, 120),  # amber family, like Erase Items
    MODE_ERASE_LIGHT_BRIDGES: QColor(245, 158, 11, 120),  # amber family, like Erase Items
    MODE_FLOOR_MATERIAL: QColor(236, 72, 153, 120),
    MODE_RAMP_MATERIAL: QColor(168, 85, 247, 120),  # purple to distinguish from floor mode pink
    MODE_ERASE: QColor(248, 113, 113, 120),
    MODE_ERASE_KEEP_FLOORS: QColor(251, 146, 60, 120),
    MODE_ERASE_LIGHTS: QColor(250, 204, 21, 120),   # amber — distinct from red erase tools
    MODE_ERASE_PRESSURE_PLATES: QColor(245, 158, 11, 120),  # amber family, like Erase Items
    MODE_ERASE_FLOORS: QColor(245, 158, 11, 120),
    MODE_ERASE_WALLS: QColor(245, 158, 11, 120),
    MODE_ERASE_BARRIERS: QColor(245, 158, 11, 120),
    MODE_ERASE_EQUIPMENT_ERASERS: QColor(187, 136, 255, 120),
    MODE_ERASE_RAMPS: QColor(245, 158, 11, 120),
    MODE_ERASE_SPAWN_ZONES: QColor(245, 158, 11, 120),
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


def materials_summary(seg: dict) -> str:
    """One-line summary of a segment's six face materials, using the same
    `all`/overrides compaction as the on-disk shape."""
    compact = compact_face_materials(seg)
    return ", ".join(f"{k}={v}" for k, v in compact.items())


def portal_label(portalable: bool) -> str:
    return "Portals allowed" if portalable else "Portals incompatible"
