from html import escape
from math import ceil, floor

from PySide6.QtCore import QPointF, QRectF, Qt
from PySide6.QtGui import QAction, QColor, QPainterPath, QPen, QPolygonF
from PySide6.QtWidgets import QComboBox, QDoubleSpinBox, QHBoxLayout, QLabel, QPushButton, QToolBar, QWidget, QWidgetAction

from .catalogs import load_map_settings, map_settings_path, read_settings_json
from .constants import GAMEPLAY_PATH, MODE_JUMP_REACH
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
        self.controls_action = QWidgetAction(window)
        self.controls = QWidget()
        controls = QHBoxLayout(self.controls)
        controls.setContentsMargins(8, 0, 0, 0)
        controls.setSpacing(6)
        self.controls_action.setDefaultWidget(self.controls)
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
        self.clear_button = QPushButton("Clear")
        self.clear_button.setAccessibleName("Clear Jump Reach")
        self.clear_button.setToolTip("Clear Jump Reach")
        self.clear_button.clicked.connect(self.clear)
        body = QWidget()
        layout = QHBoxLayout(body)
        layout.setContentsMargins(8, 0, 8, 0)
        self.legend = QLabel()
        self.legend.setTextFormat(Qt.TextFormat.RichText)
        layout.addWidget(self.legend)
        layout.addWidget(self.clear_button)
        layout.addStretch()
        self.toolbar.addWidget(body)
        self.movement.currentIndexChanged.connect(self.recompute)
        self.margin.valueChanged.connect(self.recompute)
        window.doc.changed.connect(self.document_changed)
        window.doc.replaced.connect(self.clear)
        self.reload_settings()

    def reload_settings(self):
        try:
            name = self.window.catalog_map
            self.settings = JumpSettings.from_settings(
                load_map_settings(name), str(map_settings_path(name)),
                gameplay=read_settings_json(GAMEPLAY_PATH), gameplay_source=str(GAMEPLAY_PATH),
            )
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
        self.window.canvas._clear_hover()
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
        self.controls_action.setVisible(self.window.mode == MODE_JUMP_REACH)
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
            f'<span style="color: {color}">{index + 1} {name}</span>'
            for index, (_, name, color) in enumerate(SCENARIOS)
        )
        self.legend.setText(
            f"{legend} &nbsp; ● No damage &nbsp; △ Damage &nbsp; × Fatal at full health"
        )
        self.legend.setToolTip(
            "Markers read left to right. Empty cells represent potential landing floors. "
            "Triangles survive at full health but can kill an injured player. "
            "Assumes power-ups last through landing. Hover for estimated damage as a percentage of maximum health. "
            "Ignores obstacles, ramps, body size and platform motion; assumes supporting floor behind the takeoff edge."
        )

    def hover_text(self, col: int, row: int) -> str | None:
        if self.origin is None or self.settings is None:
            return None
        key = (self.window.current_level, col, row)
        if key == self.origin:
            return "Jump Reach origin"
        landings = self.results.get(key, {})
        if not landings:
            return "Jump Reach: out of range"
        lines = ["Jump Reach:"]
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
        return "\n".join(lines)

    def paint(self, painter, cell: float):
        if self.origin is None:
            return
        canvas = self.window.canvas
        visible = canvas.viewport.visible_rect(canvas.width(), canvas.height())
        data = self.window.map_data
        level = self.window.current_level
        painter.save()
        size = min(7.0, cell * 0.16)
        for row in range(max(0, floor(visible.top())), min(data["grid_rows"], ceil(visible.bottom()))):
            for col in range(max(0, floor(visible.left())), min(data["grid_cols"], ceil(visible.right()))):
                landings = self.results.get((level, col, row), {})
                if not landings:
                    continue
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
                            painter.drawPolygon(QPolygonF([
                                QPointF(x + size / 2, y), QPointF(x + size, y + size), QPointF(x, y + size),
                            ]))
                        else:
                            cross = QPainterPath(QPointF(x, y))
                            cross.lineTo(x + size, y + size)
                            cross.moveTo(x + size, y)
                            cross.lineTo(x, y + size)
                            painter.setPen(QPen(QColor("#111418"), size * 0.45))
                            painter.drawPath(cross)
                            painter.setPen(QPen(QColor(color), size * 0.25))
                            painter.drawPath(cross)
        source_level, col, row = self.origin
        if source_level == level:
            painter.setPen(QPen(QColor("#ffffff"), 2, Qt.PenStyle.DashLine))
            painter.setBrush(Qt.BrushStyle.NoBrush)
            inset = min(3, cell * 0.1)
            painter.drawRect(QRectF(col * cell, row * cell, cell, cell).adjusted(inset, inset, -inset, -inset))
        painter.restore()
