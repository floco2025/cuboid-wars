from html import escape
from math import ceil, floor

from PySide6.QtCore import QRectF, Qt
from PySide6.QtGui import QAction, QColor, QPen
from PySide6.QtWidgets import QHBoxLayout, QLabel, QPushButton, QToolBar, QWidget

from .catalogs import load_map_settings, map_settings_path
from .constants import MODE_RUN_TIME
from .run_time import RunSettings


RUN_COLOR = "#4ade80"
SPEED_COLOR = "#fbbf24"
# Below this cell size two numbers no longer fit; hover still reports them.
MIN_LABEL_CELL = 18.0


class RunTimeOverlay:
    def __init__(self, window):
        self.window = window
        self.origin = None
        self.active_map = None
        self.settings = None
        self.error = None
        self.clear_action = QAction("Clear Run Time", window)
        self.clear_action.triggered.connect(self.clear)
        self.toolbar = QToolBar("Run Time", window)
        self.toolbar.setMovable(False)
        body = QWidget()
        layout = QHBoxLayout(body)
        layout.setContentsMargins(8, 0, 8, 0)
        self.legend = QLabel()
        self.legend.setTextFormat(Qt.TextFormat.RichText)
        self.clear_button = QPushButton("Clear")
        self.clear_button.setAccessibleName("Clear Run Time")
        self.clear_button.setToolTip("Clear Run Time")
        self.clear_button.clicked.connect(self.clear)
        layout.addWidget(self.legend)
        layout.addWidget(self.clear_button)
        layout.addStretch()
        self.toolbar.addWidget(body)
        window.doc.changed.connect(self.document_changed)
        window.doc.replaced.connect(self.clear)
        self.reload_settings()

    def reload_settings(self):
        try:
            name = self.window.catalog_map
            self.settings = RunSettings.from_settings(load_map_settings(name), str(map_settings_path(name)))
            self.error = None
        except (OSError, ValueError) as exc:
            self.settings = None
            self.error = str(exc)
            if self.origin is not None:
                self.window.notify(f"Run Time unavailable: {exc}")
        self.window.canvas._clear_hover()
        self.refresh()
        self.window.canvas.update()

    def select(self, col: int, row: int):
        self.origin = (self.window.current_level, col, row)
        self.active_map = self.window.doc.active_map
        self.window.canvas._clear_hover()
        self.refresh()
        self.window.canvas.update()

    def clear(self):
        self.origin = None
        self.window.canvas._clear_hover()
        self.refresh()
        self.window.canvas.update()

    def document_changed(self, before: dict):
        if self.origin is None:
            return
        after = self.window.map_data
        if self.active_map != self.window.doc.active_map or (
            before["grid_cols"],
            before["grid_rows"],
            len(before["levels"]),
        ) != (after["grid_cols"], after["grid_rows"], len(after["levels"])):
            self.clear()

    def refresh(self):
        active = self.origin is not None
        self.toolbar.setVisible(active or self.window.mode == MODE_RUN_TIME)
        self.clear_action.setEnabled(active)
        self.clear_button.setVisible(active)
        if self.error:
            self.legend.setText(f"Run Time unavailable: {escape(self.error)}")
            return
        self.legend.setText(
            f'<span style="color: {RUN_COLOR}">Run {self.settings.run_speed:.1f} m/s</span> &nbsp; '
            f'<span style="color: {SPEED_COLOR}">Run + Speed {self.settings.boosted_speed:.1f} m/s</span> &nbsp; '
            "seconds between cell centres"
        )
        self.legend.setToolTip(
            "Straight-line time from the origin's centre to each cell's centre on the origin's level, "
            "at run speed and with the speed power-up. "
            "Ignores walls, obstacles, ramps, ladders, jumps, and platform motion."
        )

    def hover_text(self, col: int, row: int) -> str | None:
        if self.origin is None or self.settings is None or self.window.current_level != self.origin[0]:
            return None
        if (col, row) == self.origin[1:]:
            return "Run Time origin"
        run, speed = self.settings.seconds(self.origin[1:], (col, row))
        return f"Run Time: {run:.2f} s run, {speed:.2f} s with speed"

    def paint(self, painter, cell: float):
        if self.origin is None or self.settings is None:
            return
        source_level, source_col, source_row = self.origin
        if source_level != self.window.current_level:
            return
        canvas = self.window.canvas
        visible = canvas.viewport.visible_rect(canvas.width(), canvas.height())
        data = self.window.map_data
        painter.save()
        if cell >= MIN_LABEL_CELL:
            font = painter.font()
            font.setPixelSize(round(max(7.0, min(12.0, cell * 0.28))))
            painter.setFont(font)
            half = cell / 2
            for row in range(max(0, floor(visible.top())), min(data["grid_rows"], ceil(visible.bottom()))):
                for col in range(max(0, floor(visible.left())), min(data["grid_cols"], ceil(visible.right()))):
                    if (col, row) == (source_col, source_row):
                        continue
                    run, speed = self.settings.seconds((source_col, source_row), (col, row))
                    for text, color, top in (
                        (f"{run:.1f}", RUN_COLOR, row * cell),
                        (f"{speed:.1f}", SPEED_COLOR, row * cell + half),
                    ):
                        rect = QRectF(col * cell, top, cell, half)
                        painter.setPen(QColor("#111418"))
                        painter.drawText(rect.translated(1, 1), Qt.AlignmentFlag.AlignCenter, text)
                        painter.setPen(QColor(color))
                        painter.drawText(rect, Qt.AlignmentFlag.AlignCenter, text)
        painter.setPen(QPen(QColor("#ffffff"), 2, Qt.PenStyle.DashLine))
        painter.setBrush(Qt.BrushStyle.NoBrush)
        inset = min(3, cell * 0.1)
        painter.drawRect(
            QRectF(source_col * cell, source_row * cell, cell, cell).adjusted(inset, inset, -inset, -inset)
        )
        painter.restore()
