"""Draw the selection and its active gesture without inventing another target."""

from PySide6.QtCore import QPointF, QRectF, Qt
from PySide6.QtGui import QColor, QPen

from . import constants as c
from .elements import ELEMENT_MODES
from .geometry import rect_from_cells
from .ladder_glyph import ladder_marker_lines
from .transforms import record_levels, record_rect


def paint_outline(painter, name, entry, cell, offset=(0, 0)):
    if name == "ladders":
        ladder = {**entry, "col": entry["col"] + offset[0], "row": entry["row"] + offset[1]}
        for x0, y0, x1, y1 in ladder_marker_lines(ladder, cell):
            painter.drawLine(round(x0), round(y0), round(x1), round(y1))
        return
    x0, y0, x1, y1 = record_rect(name, entry)
    x0, x1 = x0 + offset[0], x1 + offset[0]
    y0, y1 = y0 + offset[1], y1 + offset[1]
    bounds = QRectF(x0 * cell, y0 * cell, (x1 - x0) * cell, (y1 - y0) * cell)
    if name in ("items", "pressure_plates", "lights"):
        inset = cell * 0.2
        painter.drawEllipse(bounds.adjusted(inset, inset, -inset, -inset))
    elif x0 == x1 or y0 == y1:
        painter.drawLine(bounds.topLeft(), bounds.bottomRight())
    else:
        inset = min(2, cell * 0.1)
        painter.drawRect(bounds.adjusted(inset, inset, -inset, -inset))


def paint_selection(canvas, painter, cell):
    window = canvas.window
    selection = window.selection
    gesture = canvas.input.gesture
    line = canvas.input.line_selection()
    painter.save()
    painter.setPen(QPen(QColor("#38bdf8"), 2))
    painter.setBrush(Qt.BrushStyle.NoBrush)
    if selection.area is not None:
        c0, r0, c1, r1 = selection.area.rect
        painter.setBrush(QColor(56, 189, 248, 40))
        painter.drawRect(QRectF(c0 * cell, r0 * cell, (c1 - c0) * cell, (r1 - r0) * cell))
    else:
        for ref in selection.objects:
            entry = ref.get(window.map_data)
            if ref.name == "nested_maps":
                if window.pending_block is None and (gesture is None or gesture.kind != "handle"):
                    canvas.paint_nested_map(painter, entry, cell, window.current_level, color=QColor("#38bdf8"))
                continue
            lower, upper = record_levels(entry, ref.level)
            if lower <= window.current_level <= upper:
                paint_outline(painter, ref.name, entry, cell)
    if gesture is not None and gesture.kind in ("box", "add_box") and gesture.moved:
        painter.setPen(QPen(QColor("#38bdf8"), 1, Qt.PenStyle.DashLine))
        if line is not None:
            x0, y0, x1, y1 = line[0]
            painter.drawLine(QPointF(x0 * cell, y0 * cell), QPointF(x1 * cell, y1 * cell))
        else:
            c0, r0, c1, r1 = rect_from_cells(
                canvas.input.clamped_cell(gesture.start), canvas.input.clamped_cell(gesture.current)
            )
            painter.setBrush(QColor(56, 189, 248, 35) if window.selection_kind == "Tiles" else Qt.BrushStyle.NoBrush)
            painter.drawRect(QRectF(c0 * cell, r0 * cell, (c1 - c0) * cell, (r1 - r0) * cell))
    painter.setBrush(QColor("#f1f5f9"))
    painter.setPen(QPen(QColor("#0f172a"), 1))
    size = c.SPAWN_ZONE_HANDLE_PIXELS
    for handle in canvas.input.handles():
        if handle.ref.name == "nested_maps" and window.pending_block is not None:
            continue
        x, y = handle.point
        if (
            handle.ref.name == "nested_maps"
            and gesture is not None
            and gesture.kind == "handle"
            and handle == gesture.handle
        ):
            col, row = canvas.input.clamped_cell(gesture.current)
            x, y = col + 0.5, row + 0.5
        painter.drawRect(QRectF(x * cell - size / 2, y * cell - size / 2, size, size))
    if gesture is not None and gesture.kind == "handle":
        rect = canvas.input.zone_preview()
        if rect is not None:
            c0, r0, c1, r1 = rect
            if "kind" in gesture.original:
                canvas.paint_roam_range(
                    painter, {**gesture.original, "cols": [c0, c1], "rows": [r0, r1]}, cell, window.current_level
                )
            painter.setBrush(QColor(248, 250, 252, 40))
            painter.setPen(QPen(QColor("#f8fafc"), 2, Qt.PenStyle.DashLine))
            painter.drawRect(QRectF(c0 * cell, r0 * cell, (c1 - c0) * cell, (r1 - r0) * cell))
        else:
            col, row = canvas.input.clamped_cell(gesture.current)
            entry = {**gesture.original, gesture.handle.name: [col, row]}
            canvas.paint_nested_map(painter, entry, cell, window.current_level, color=QColor("#f8fafc"))
    if gesture is not None and gesture.kind in ("box", "add_box") and gesture.moved:
        name = line[1][0].name if line is not None and line[1] else gesture.object_type
        if name is not None:
            verb = "Add" if gesture.kind == "add_box" else "Select"
            paint_caption(canvas, painter, f"{verb}: {ELEMENT_MODES[name]}")
    painter.restore()


def paint_caption(canvas, painter, text):
    width, height = painter.fontMetrics().horizontalAdvance(text) + 16, painter.fontMetrics().height() + 8
    rect = QRectF(
        8 - canvas.viewport.offset.x(), canvas.height() - canvas.viewport.offset.y() - height - 8, width, height
    )
    painter.fillRect(rect, QColor("#111418"))
    painter.setPen(QColor("#f1f5f9"))
    painter.drawText(rect, Qt.AlignmentFlag.AlignCenter, text)
