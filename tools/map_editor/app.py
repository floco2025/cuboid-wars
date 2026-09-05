"""Command-line entry point for the map editor."""

from __future__ import annotations

import argparse
import re
import signal
import sys

from PySide6.QtCore import QPointF, QRectF, Qt
from PySide6.QtGui import QBrush, QColor, QIcon, QPainter, QPen, QPixmap, QPolygonF
from PySide6.QtWidgets import QApplication

from .constants import MAPS_DIR
from .window import EditorWindow

# Same rule the server enforces on map names in gameplay.json.
MAP_NAME_RE = re.compile(r"^[A-Za-z0-9_-]+$")


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
    parser.add_argument("map", help="Map name to edit (opens config/server/maps/<name>.json).")
    args = parser.parse_args()
    if not MAP_NAME_RE.match(args.map):
        parser.error(f"invalid map name {args.map!r}: use only ASCII letters, digits, '_', or '-'")
    map_path = MAPS_DIR / f"{args.map}.json"
    if not map_path.exists():
        print(f"map '{args.map}' has no file yet; Save will create {map_path}", file=sys.stderr)

    app = QApplication(sys.argv)
    # Used by QSettings for the recent-files list — both must be set so the
    # settings land in a predictable, per-OS location.
    app.setOrganizationName("CuboidWars")
    app.setApplicationName("MapEditor")
    app.setWindowIcon(_build_window_icon())
    # Ctrl-C ends the process outright: a Python-level handler could only
    # ask the main event loop to quit, which a modal dialog's nested loop
    # never gets around to.
    signal.signal(signal.SIGINT, signal.SIG_DFL)

    window = EditorWindow(map_path)
    window.show()
    return app.exec()
