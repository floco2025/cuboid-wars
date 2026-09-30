"""Draws the Jump Path overlay: flights, landings, regions, portals, and input markers."""

from PySide6.QtCore import QPointF, QRectF, Qt
from PySide6.QtGui import QColor, QPainter, QPen, QPolygonF

from .jump_path import Takeoff
from .reach_markers import OUTLINE, paint_landing_glyph

REGION_STYLES = {
    "capture": Qt.PenStyle.SolidLine,
    "capture_steered": Qt.PenStyle.DashLine,
    "range": Qt.PenStyle.DotLine,
}
ENTRY_COLOR = "#60a5fa"
EXIT_COLOR = "#fb923c"
UNAVAILABLE_COLOR = "#ef4444"
PREVIEW_COLOR = "#ffffff"
FLIGHT_COLOR = "#4ade80"
ELSEWHERE_COLOR = "#9ca3af"


def paint_jump_path(overlay, painter, cell):
    settings = overlay.settings
    if settings is None or not (overlay.has_selection or overlay.preview is not None):
        return
    level = overlay.window.current_level
    scale = cell / settings.cell_size
    painter.save()
    painter.setRenderHint(QPainter.RenderHint.Antialiasing)
    view = overlay.view(level)
    if view is not None:
        _paint_regions(painter, scale, overlay.regions(level))
        _paint_runs(painter, scale, view.runs)
        _paint_glyphs(painter, cell, scale, view.glyphs)
        reentry = overlay.reentry()
        if reentry is not None and reentry[0].level == level:
            _paint_fall_back(painter, cell, scale, reentry[0].frame(settings).center)
    _paint_shooters(overlay, painter, cell, level)
    # On other levels the takeoff still shows where the path comes from.
    if overlay.takeoff is not None:
        _paint_takeoff(
            painter, cell, overlay.takeoff, PREVIEW_COLOR if overlay.takeoff.level == level else ELSEWHERE_COLOR
        )
    if isinstance(overlay.preview, Takeoff):
        _paint_takeoff(painter, cell, overlay.preview, ELSEWHERE_COLOR)
    for surface, color, label in (
        (overlay.entry, ENTRY_COLOR, "1"),
        (overlay.exit, EXIT_COLOR, "2"),
        (None if isinstance(overlay.preview, Takeoff) else overlay.preview, PREVIEW_COLOR, ""),
    ):
        if surface is not None and surface.level == level:
            status = overlay.status(surface)
            _paint_surface(
                overlay, painter, cell, surface, color if status.available else UNAVAILABLE_COLOR, label, status.planned
            )
    painter.restore()


def _flight_color(alpha):
    color = QColor(FLIGHT_COLOR)
    color.setAlpha(alpha)
    return color


def _paint_regions(painter, scale, regions):
    painter.setBrush(Qt.BrushStyle.NoBrush)
    for region in regions:
        painter.setPen(QPen(_flight_color(200), 1.5, REGION_STYLES[region.kind]))
        for (ax, az), (bx, bz) in region.segments:
            painter.drawLine(QPointF(ax * scale, az * scale), QPointF(bx * scale, bz * scale))


def _paint_runs(painter, scale, runs):
    painter.setBrush(Qt.BrushStyle.NoBrush)
    for run in runs:
        painter.setPen(QPen(_flight_color(235 if run.inside else 90), 2.5 if run.inside else 1))
        painter.drawPolyline(QPolygonF([QPointF(x * scale, z * scale) for x, z in run.points]))


def _paint_glyphs(painter, cell, scale, glyphs):
    size = max(6.0, min(10.0, cell * 0.22))
    for glyph in glyphs:
        paint_landing_glyph(
            painter,
            QPointF(glyph.point[0] * scale, glyph.point[1] * scale),
            size,
            _flight_color(110 if glyph.blocked else 255),
            glyph.damage,
            hollow=not glyph.supported,
        )


# A turning arrow on the portal a flight falls back into.
def _paint_fall_back(painter, cell, scale, center):
    x, z = center[0] * scale, center[2] * scale
    radius = max(7.0, cell * 0.16)
    painter.setPen(QPen(QColor(FLIGHT_COLOR), 2.5))
    painter.setBrush(Qt.BrushStyle.NoBrush)
    # Qt measures arcs in sixteenths of a degree, anticlockwise from three o'clock.
    painter.drawArc(QRectF(x - radius, z - radius, 2 * radius, 2 * radius), 60 * 16, 270 * 16)
    tip = QPointF(x + radius * 0.866, z + radius * 0.5)
    for dx, dz in ((-0.75, 0.1), (0.05, 0.75)):
        painter.drawLine(tip, tip + QPointF(dx, dz) * radius * 0.7)


def _paint_takeoff(painter, cell, takeoff, color):
    x, z = (value * cell for value in takeoff.grid_point)
    dx, dz = takeoff.direction
    half = max(5.0, cell * 0.14)
    length = max(9.0, cell * 0.3)
    tip = QPointF(x + dx * length, z + dz * length)
    painter.setPen(QPen(QColor(OUTLINE), 5))
    painter.drawLine(QPointF(x - dz * half, z + dx * half), QPointF(x + dz * half, z - dx * half))
    painter.setPen(QPen(QColor(color), 3))
    painter.drawLine(QPointF(x - dz * half, z + dx * half), QPointF(x + dz * half, z - dx * half))
    painter.setPen(QPen(QColor(color), 2))
    painter.drawLine(QPointF(x, z), tip)
    for sign in (-1, 1):
        painter.drawLine(tip, tip - QPointF(dx - sign * dz, dz + sign * dx) * length * 0.3)


def _paint_shooters(overlay, painter, cell, level):
    for position, label, color in ((overlay.entry_shot, "S1", ENTRY_COLOR), (overlay.exit_shot, "S2", EXIT_COLOR)):
        if position is None or position[0] != level:
            continue
        center = QPointF(position[1] * cell, position[2] * cell)
        painter.setPen(QPen(QColor(color), 2))
        painter.setBrush(QColor(OUTLINE))
        painter.drawEllipse(center, 4, 4)
        painter.drawText(QRectF(center.x() + 6, center.y() - 16, 24, 14), Qt.AlignmentFlag.AlignLeft, label)


def _paint_surface(overlay, painter, cell, surface, color, label, planned):
    settings = overlay.settings
    frame = surface.frame(settings)
    scale = cell / settings.cell_size
    x, z = frame.center[0] * scale, frame.center[2] * scale
    direction = frame.up if surface.face == "floor" else frame.normal
    painter.setPen(QPen(QColor(color), 2 if label else 1, Qt.PenStyle.DashLine if planned else Qt.PenStyle.SolidLine))
    painter.setBrush(Qt.BrushStyle.NoBrush)
    if surface.face == "floor":
        rx, rz = (
            (settings.portal_half_width, settings.portal_half_height)
            if surface.turn % 2 == 0
            else (settings.portal_half_height, settings.portal_half_width)
        )
        painter.drawEllipse(QRectF(x - rx * scale, z - rz * scale, 2 * rx * scale, 2 * rz * scale))
    else:
        dx, dz = (
            frame.right[0] * settings.portal_half_width * scale,
            frame.right[2] * settings.portal_half_width * scale,
        )
        painter.drawLine(QPointF(x - dx, z - dz), QPointF(x + dx, z + dz))
    length = cell * (0.28 if label else 0.16)
    tip = QPointF(x + direction[0] * length, z + direction[2] * length)
    painter.drawLine(QPointF(x, z), tip)
    for sign in (-1, 1):
        painter.drawLine(
            tip,
            tip - QPointF(direction[0] - sign * direction[2], direction[2] + sign * direction[0]) * length * 0.3,
        )
    if label:
        badge = QRectF(x + cell * 0.13, z - cell * 0.3, max(14, cell * 0.2), max(14, cell * 0.2))
        painter.setBrush(QColor(OUTLINE))
        painter.drawRoundedRect(badge, 3, 3)
        painter.drawText(badge, Qt.AlignmentFlag.AlignCenter, label)
