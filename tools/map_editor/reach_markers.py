"""Landing glyphs and damage descriptions."""

from PySide6.QtCore import QPointF, QRectF, Qt
from PySide6.QtGui import QColor, QPainterPath, QPen, QPolygonF

OUTLINE = "#111418"


# `damage` is a fraction of full health.
def landing_line(damage: float) -> str:
    if damage == 0:
        return "● No damage"
    if damage == 1:
        return "× Fatal at full health"
    percent = damage * 100
    amount = "<0.1" if percent < 0.1 else ">99.9" if percent > 99.9 else f"{percent:.1f}"
    return f"△ Damage: {amount}% of full health"


# A dot lands safely, a triangle with damage, a cross fatally. `hollow` marks a
# landing with no floor under it: an outline inside a dashed ring.
def paint_landing_glyph(painter, center: QPointF, size: float, color: QColor, damage: float, *, hollow: bool):
    dark = QColor(OUTLINE)
    dark.setAlpha(color.alpha())
    x, y = center.x() - size / 2, center.y() - size / 2
    if hollow:
        painter.setPen(QPen(color, 1, Qt.PenStyle.DashLine))
        painter.setBrush(Qt.BrushStyle.NoBrush)
        painter.drawEllipse(center, size, size)
    if damage == 0:
        painter.setPen(QPen(color if hollow else dark, max(1.0, size * 0.15)))
        painter.setBrush(dark if hollow else color)
        painter.drawEllipse(QRectF(x, y, size, size))
    elif damage < 1:
        painter.setPen(QPen(color, size * 0.2))
        painter.setBrush(dark)
        painter.drawPolygon(QPolygonF([QPointF(x + size / 2, y), QPointF(x + size, y + size), QPointF(x, y + size)]))
    else:
        cross = QPainterPath(QPointF(x, y))
        cross.lineTo(x + size, y + size)
        cross.moveTo(x + size, y)
        cross.lineTo(x, y + size)
        painter.setPen(QPen(dark, size * 0.45))
        painter.drawPath(cross)
        painter.setPen(QPen(color, size * 0.25))
        painter.drawPath(cross)
