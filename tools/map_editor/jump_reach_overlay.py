from html import escape
from math import ceil, floor

from PySide6.QtCore import QRectF, Qt
from PySide6.QtGui import QAction, QColor, QPen
from PySide6.QtWidgets import QComboBox, QDoubleSpinBox, QHBoxLayout, QLabel, QPushButton, QToolBar, QVBoxLayout, QWidget

from .catalogs import load_map_settings, map_settings_path
from .constants import MODE_JUMP_REACH
from .floor_footprints import corner_filler_skips, slab_cells
from .jump_reach import ANTI_GRAVITY, BOTH, NORMAL, SPEED, JumpSettings, calculate_reach


SCENARIOS = (
    (NORMAL, "Normal", "#4ade80"),
    (SPEED, "Speed", "#fbbf24"),
    (ANTI_GRAVITY, "Anti-gravity", "#60a5fa"),
    (BOTH, "Both", "#e879f9"),
)


class JumpReachOverlay:
    def __init__(self, window):
        self.window = window
        self.origin = None
        self.active_map = None
        self.results = {}
        self.settings = None
        self.error = None
        self.clear_action = QAction("Clear Jump Reach", window)
        self.clear_action.triggered.connect(self.clear)
        self.toolbar = QToolBar("Jump Reach", window)
        self.toolbar.setMovable(False)
        body = QWidget()
        layout = QVBoxLayout(body)
        layout.setContentsMargins(8, 3, 8, 3)
        layout.setSpacing(3)
        controls = QHBoxLayout()
        controls.addWidget(QLabel("Jump Reach"))
        self.movement = QComboBox()
        self.movement.addItems(["Run", "Walk"])
        self.movement.setAccessibleName("Jump movement")
        controls.addWidget(self.movement)
        caption = QLabel("Takeoff margin")
        self.margin = QDoubleSpinBox()
        self.margin.setDecimals(3)
        self.margin.setRange(0, 999.999)
        self.margin.setSingleStep(0.01)
        self.margin.setValue(0.1)
        self.margin.setSuffix(" s")
        self.margin.setKeyboardTracking(False)
        self.margin.setAccessibleName("Takeoff margin")
        self.margin.setToolTip("Jump this much travel time before the edge, allowing for edge judgement and button timing.")
        caption.setBuddy(self.margin)
        controls.addWidget(caption)
        controls.addWidget(self.margin)
        self.distance = QLabel()
        controls.addWidget(self.distance)
        self.clear_button = QPushButton("Clear Jump Reach")
        self.clear_button.clicked.connect(self.clear)
        controls.addWidget(self.clear_button)
        controls.addStretch()
        layout.addLayout(controls)
        self.legend = QLabel()
        self.legend.setTextFormat(Qt.TextFormat.RichText)
        self.legend.setWordWrap(True)
        layout.addWidget(self.legend)
        self.toolbar.addWidget(body)
        self.movement.currentIndexChanged.connect(self.recompute)
        self.margin.valueChanged.connect(self.recompute)
        window.doc.changed.connect(self.document_changed)
        window.doc.replaced.connect(self.clear)
        self.reload_settings()

    def reload_settings(self):
        try:
            name = self.window.catalog_map
            self.settings = JumpSettings.from_settings(load_map_settings(name), str(map_settings_path(name)))
            self.error = None
        except (OSError, ValueError) as exc:
            self.settings = None
            self.error = str(exc)
            if self.origin is not None:
                self.window.notify(f"Jump Reach unavailable: {exc}")
        self.recompute()

    def select(self, col: int, row: int):
        self.origin = (self.window.current_level, col, row)
        self.active_map = self.window.doc.active_map
        self.recompute()

    def clear(self):
        self.origin = None
        self.results = {}
        self.window.canvas._clear_hover()
        self.refresh()
        self.window.canvas.update()

    def document_changed(self, before: dict):
        if self.origin is None:
            return
        after = self.window.map_data
        if (
            self.active_map != self.window.doc.active_map
            or (before["grid_cols"], before["grid_rows"], len(before["levels"]))
            != (after["grid_cols"], after["grid_rows"], len(after["levels"]))
        ):
            self.clear()
        elif slab_cells(before) != slab_cells(after) or corner_filler_skips(before) != corner_filler_skips(after):
            self.recompute()

    def recompute(self):
        self.results = {}
        if self.origin is not None and self.settings is not None:
            data = self.window.map_data
            self.results = calculate_reach(
                self.settings, self.origin, data,
                running=self.movement.currentText() == "Run", margin=self.margin.value(),
            )
        self.refresh()
        self.window.canvas.update()

    def refresh(self):
        active = self.origin is not None
        self.toolbar.setVisible(active or self.window.mode == MODE_JUMP_REACH)
        self.clear_action.setEnabled(active)
        self.clear_button.setVisible(active)
        if self.settings is not None:
            distance = self.settings.speed(self.movement.currentText() == "Run") * self.margin.value()
            self.distance.setText(f"{distance:.2f} m normal / {distance * self.settings.speed_multiplier:.2f} m speed")
        else:
            self.distance.clear()
        if self.error:
            self.legend.setText(f"Jump Reach unavailable: {escape(self.error)}")
            return
        legend = " &nbsp; ".join(
            f'<span style="color: {color}">{index + 1} ● {name}</span>'
            for index, (_, name, color) in enumerate(SCENARIOS)
        )
        source = "Click any cell to set the origin."
        if active:
            level, col, row = self.origin
            source = f"Origin: level {level}, cell ({col}, {row})."
        self.legend.setText(f"{legend} &nbsp; {source} Open-space estimate.")
        self.legend.setToolTip(
            "Markers read left to right. Empty cells represent potential landing floors. "
            "Ignores obstacles, ramps, body size and platform motion; assumes supporting floor behind the takeoff edge."
        )

    def hover_text(self, col: int, row: int) -> str | None:
        if self.origin is None or self.settings is None:
            return None
        key = (self.window.current_level, col, row)
        if key == self.origin:
            return "Jump Reach origin"
        mask = self.results.get(key, 0)
        names = [name for bit, name, _ in SCENARIOS if mask & bit]
        return "Jump Reach: " + (", ".join(names) if names else "out of range")

    def paint(self, painter, cell: float):
        if self.origin is None:
            return
        canvas = self.window.canvas
        visible = canvas.viewport.visible_rect(canvas.width(), canvas.height())
        data = self.window.map_data
        level = self.window.current_level
        painter.save()
        size = min(7.0, cell * 0.14)
        for row in range(max(0, floor(visible.top())), min(data["grid_rows"], ceil(visible.bottom()))):
            for col in range(max(0, floor(visible.left())), min(data["grid_cols"], ceil(visible.right()))):
                mask = self.results.get((level, col, row), 0)
                if not mask:
                    continue
                for index, (bit, _, color) in enumerate(SCENARIOS):
                    if mask & bit:
                        painter.setPen(QPen(QColor("#111418"), min(1.0, size * 0.15)))
                        painter.setBrush(QColor(color))
                        x = (col + 0.2 + index * 0.2) * cell - size / 2
                        y = (row + 0.78) * cell - size / 2
                        painter.drawEllipse(QRectF(x, y, size, size))
        source_level, col, row = self.origin
        if source_level == level:
            painter.setPen(QPen(QColor("#ffffff"), 2, Qt.PenStyle.DashLine))
            painter.setBrush(Qt.BrushStyle.NoBrush)
            inset = min(3, cell * 0.1)
            painter.drawRect(QRectF(col * cell, row * cell, cell, cell).adjusted(inset, inset, -inset, -inset))
        painter.restore()
