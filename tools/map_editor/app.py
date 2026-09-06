"""Command-line entry point for the map editor."""

from __future__ import annotations

import argparse
import signal
import sys

from PySide6.QtCore import QPointF, QTimer, Qt
from PySide6.QtGui import QBrush, QColor, QIcon, QPainter, QPen, QPixmap, QPolygonF
from PySide6.QtWidgets import QApplication

from .constants import MAP_NAME_RE, MAPS_DIR
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
    parser.add_argument("map", help="Map name to edit (opens config/server/maps/<name>.json).")
    args = parser.parse_args()
    if not MAP_NAME_RE.match(args.map):
        parser.error(f"invalid map name {args.map!r}: use only ASCII letters, digits, '_', or '-'")
    map_path = MAPS_DIR / f"{args.map}.json"
    if not map_path.exists():
        print(f"map '{args.map}' has no file yet; Save will create {map_path}", file=sys.stderr)

    app = QApplication(sys.argv)
    # Both names keep editor preferences in a predictable, per-OS location.
    app.setOrganizationName("CuboidWars")
    app.setApplicationName("MapEditor")
    app.setWindowIcon(_build_window_icon())
    # Ctrl-C ends the process outright. A Python-level handler could only
    # ask the main event loop to quit, which a modal dialog's nested loop
    # never gets around to; the autosave has already kept the work.
    signal.signal(signal.SIGINT, signal.SIG_DFL)

    window = EditorWindow(map_path)
    window.show()
    window.raise_()
    window.activateWindow()
    # Once the window is up and the app is in front, so the prompt lands on
    # top of it.
    QTimer.singleShot(0, window.maybe_recover_autosave)
    return app.exec()
