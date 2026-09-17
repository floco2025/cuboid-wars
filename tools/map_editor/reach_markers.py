"""Shared scenario colors, landing glyphs, and damage descriptions."""

from PySide6.QtCore import QPointF, QRectF
from PySide6.QtGui import QColor, QPainterPath, QPen, QPolygonF

from .jump_reach import ANTI_GRAVITY, BOTH, NORMAL, SPEED

SCENARIOS = (
    (NORMAL, "Normal", "#4ade80"),
    (SPEED, "Speed", "#fbbf24"),
    (ANTI_GRAVITY, "Anti-gravity", "#60a5fa"),
    (BOTH, "Both", "#e879f9"),
)


def landing_lines(landings):
    lines = []
    for bit, name, _ in SCENARIOS:
        if bit not in landings:
            continue
        damage = landings[bit]
        if damage == 0:
            lines.append(f"● {name}")
            continue
        if damage == 1:
            symbol = "×"
            amount = "100"
        else:
            symbol = "△"
            percent = damage * 100
            amount = "<0.1" if percent < 0.1 else ">99.9" if percent > 99.9 else f"{percent:.1f}"
        lines.append(f"{symbol} {name} (damage: {amount}%)")
    return lines


def paint_landing_markers(painter, cell, col, row, landings):
    size = min(7.0, cell * 0.16)
    for index, (bit, _, color) in enumerate(SCENARIOS):
        if bit in landings:
            painter.setPen(QPen(QColor("#111418"), min(1.0, size * 0.15)))
            painter.setBrush(QColor(color))
            x = (col + 0.2 + index * 0.2) * cell - size / 2
            y = (row + 0.78) * cell - size / 2
            if landings[bit] == 0:
                painter.drawEllipse(QRectF(x, y, size, size))
            elif landings[bit] < 1:
                painter.setPen(QPen(QColor(color), size * 0.2))
                painter.setBrush(QColor("#111418"))
                painter.drawPolygon(
                    QPolygonF(
                        [
                            QPointF(x + size / 2, y),
                            QPointF(x + size, y + size),
                            QPointF(x, y + size),
                        ]
                    )
                )
            else:
                cross = QPainterPath(QPointF(x, y))
                cross.lineTo(x + size, y + size)
                cross.moveTo(x + size, y)
                cross.lineTo(x, y + size)
                painter.setPen(QPen(QColor("#111418"), size * 0.45))
                painter.drawPath(cross)
                painter.setPen(QPen(QColor(color), size * 0.25))
                painter.drawPath(cross)
