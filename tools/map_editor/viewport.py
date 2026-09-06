"""One grid-to-canvas transform for drawing, picking, zoom, and pan."""

from __future__ import annotations

from dataclasses import dataclass, field

from PySide6.QtCore import QPointF, QRectF


@dataclass
class Viewport:
    cell: float = 36.0
    offset: QPointF = field(default_factory=QPointF)
    fitted: bool = True

    def fit(self, width: float, height: float, cols: int, rows: int) -> None:
        self.cell = max(0.5, min(width / max(1, cols), height / max(1, rows)))
        self.offset = QPointF()
        self.fitted = True

    def to_grid(self, point: QPointF) -> QPointF:
        return (point - self.offset) / self.cell

    def from_grid(self, point: QPointF) -> QPointF:
        return point * self.cell + self.offset

    def zoom(self, factor: float, anchor: QPointF) -> None:
        grid = self.to_grid(anchor)
        self.cell = max(0.5, min(192.0, self.cell * factor))
        self.offset = anchor - grid * self.cell
        self.fitted = False

    def pan(self, delta: QPointF) -> None:
        self.offset += delta
        self.fitted = False

    def visible_rect(self, width: float, height: float) -> QRectF:
        return QRectF(self.to_grid(QPointF()), self.to_grid(QPointF(width, height)))

    def focus(self, rect: tuple, width: float, height: float) -> None:
        c0, r0, c1, r1 = rect
        self.cell = max(12.0, min(64.0, width / max(4, c1 - c0 + 2), height / max(4, r1 - r0 + 2)))
        self.offset = QPointF(width / 2, height / 2) - QPointF((c0 + c1) / 2, (r0 + r1) / 2) * self.cell
        self.fitted = False
