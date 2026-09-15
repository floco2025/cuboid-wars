"""Shared ladder glyph for drawing, selection, and move previews."""

# Ladder glyph proportions, in cell units. The glyph hugs the anchor edge
# on the ladder's rail side: two rails parallel to the edge plus rungs
# between them — a ladder seen face-on.
_LADDER_SPAN = (0.15, 0.85)  # extent along the edge
_LADDER_NEAR = 0.04  # rail offsets from the edge
_LADDER_FAR = 0.26
_LADDER_RUNG_COUNT = 4


def ladder_marker_lines(ladder: dict, cell: float) -> list[tuple[float, float, float, float]]:
    """Line segments (x0, y0, x1, y1) in pixels for a ladder's canvas glyph."""
    col, row, side = ladder["col"], ladder["row"], ladder["side"]
    if side == "N":
        origin, edge, normal = (col, row), (1, 0), (0, -1)
    elif side == "S":
        origin, edge, normal = (col, row + 1), (1, 0), (0, 1)
    elif side == "W":
        origin, edge, normal = (col, row), (0, 1), (-1, 0)
    else:  # "E"
        origin, edge, normal = (col + 1, row), (0, 1), (1, 0)
    ox, oy = origin
    ex, ey = edge
    nx, ny = normal
    t0, t1 = _LADDER_SPAN

    def point(t: float, off: float) -> tuple[float, float]:
        return ((ox + ex * t + nx * off) * cell, (oy + ey * t + ny * off) * cell)

    lines = []
    for off in (_LADDER_NEAR, _LADDER_FAR):
        lines.append((*point(t0, off), *point(t1, off)))
    for idx in range(_LADDER_RUNG_COUNT):
        t = t0 + (t1 - t0) * (idx + 0.5) / _LADDER_RUNG_COUNT
        lines.append((*point(t, _LADDER_NEAR), *point(t, _LADDER_FAR)))
    return lines
