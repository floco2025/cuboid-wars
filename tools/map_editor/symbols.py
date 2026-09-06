"""Item silhouettes shared with the client meshes and HUD."""

import json

from PySide6.QtCore import QPointF, Qt
from PySide6.QtGui import QColor, QPainterPath, QPen, QPolygonF

from .constants import REPO_ROOT

with (REPO_ROOT / "client/assets/symbols/items.json").open(encoding="utf-8") as handle:
    ITEM_SYMBOLS = json.load(handle)


def paint_item_symbol(painter, item_type, cx, cy, size, color):
    path = QPainterPath()
    path.setFillRule(Qt.FillRule.WindingFill)
    symbol = ITEM_SYMBOLS[item_type]
    for polygon in symbol.get("polygons", []):
        path.addPolygon(QPolygonF([QPointF(cx + x * size, cy - y * size) for x, y in polygon]))
        path.closeSubpath()
    for circle in symbol.get("circles", []):
        x, y = circle["center"]
        radius = circle["radius"] * size
        path.addEllipse(QPointF(cx + x * size, cy - y * size), radius, radius)
    painter.save()
    painter.setBrush(color)
    painter.setPen(QPen(QColor("#0f172a"), 0.8))
    painter.drawPath(path.simplified())
    painter.restore()
