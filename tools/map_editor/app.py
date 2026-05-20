"""Command-line entry point for the map editor."""

from __future__ import annotations

import argparse
import signal
import sys
from pathlib import Path

from PySide6.QtCore import QPointF, QRectF, Qt, QTimer
from PySide6.QtGui import QBrush, QColor, QIcon, QPainter, QPen, QPixmap, QPolygonF
from PySide6.QtWidgets import QApplication

from .constants import DEFAULT_MAP
from .window import EditorWindow


def _build_window_icon() -> QIcon:
    # Render a small isometric cuboid into a QPixmap so the editor has a
    # recognizable taskbar/dock icon without shipping a binary asset.
    pixmap = QPixmap(64, 64)
    pixmap.fill(Qt.GlobalColor.transparent)
    painter = QPainter(pixmap)
    painter.setRenderHint(QPainter.RenderHint.Antialiasing)
    painter.setPen(QPen(QColor("#0f172a"), 2))
    top = QPolygonF([QPointF(32, 8), QPointF(56, 22), QPointF(32, 36), QPointF(8, 22)])
    painter.setBrush(QBrush(QColor("#fbbf24")))
    painter.drawPolygon(top)
    left = QPolygonF([QPointF(8, 22), QPointF(32, 36), QPointF(32, 58), QPointF(8, 44)])
    painter.setBrush(QBrush(QColor("#b45309")))
    painter.drawPolygon(left)
    right = QPolygonF([QPointF(32, 36), QPointF(56, 22), QPointF(56, 44), QPointF(32, 58)])
    painter.setBrush(QBrush(QColor("#d97706")))
    painter.drawPolygon(right)
    painter.end()
    return QIcon(pixmap)


def main() -> int:
    parser = argparse.ArgumentParser(description="Cuboid Wars map editor.")
    parser.add_argument("file", nargs="?", type=Path, default=DEFAULT_MAP, help="Map JSON to edit.")
    args = parser.parse_args()

    app = QApplication(sys.argv)
    # Used by QSettings for the recent-files list — both must be set so the
    # settings land in a predictable, per-OS location.
    app.setOrganizationName("CuboidWars")
    app.setApplicationName("MapEditor")
    app.setWindowIcon(_build_window_icon())
    signal.signal(signal.SIGINT, lambda _signum, _frame: app.exit(130))
    sigint_timer = QTimer()
    sigint_timer.timeout.connect(lambda: None)
    sigint_timer.start(100)

    window = EditorWindow(args.file)
    window.show()
    return app.exec()
