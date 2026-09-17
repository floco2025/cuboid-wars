from html import escape
from math import ceil, floor

from PySide6.QtCore import QPointF, QRectF, Qt
from PySide6.QtGui import QAction, QColor, QPen
from PySide6.QtWidgets import (
    QComboBox,
    QDoubleSpinBox,
    QHBoxLayout,
    QLabel,
    QPushButton,
    QSizePolicy,
    QToolBar,
    QWidget,
    QWidgetAction,
)

from . import constants
from .catalogs import load_map_settings, map_settings_path, read_settings_json
from .compact_widgets import CompactComboBox
from .portal_jump import PORTAL_HALF_HEIGHT, PORTAL_HALF_WIDTH, PortalSettings, calculate_landings, entry_states
from .portal_surfaces import PortalSurfaces, portals_overlap
from .reach_markers import SCENARIOS, landing_lines, paint_landing_markers

LIMITS = (
    "Open-space estimate through portal centers; air steering can stop or change direction immediately. "
    "Each portal is aimed from its shooting position, initially the jump position; floor orientations follow the game's quarter-turn snap. "
    "Assumes sufficient backing: floors are centered on tiles; wall portals' lower rims align with the wall base. "
    "Ignores intervening obstacles, moving platforms, front clearance, placement nudging, fixtures, funnel assistance and repeat crossings. "
    "Power-ups last throughout. Dashed outlines indicate planned surfaces. "
    "J marks the jump position; S1 and S2 mark the two shooting positions. "
    "Landing symbols: ● safe, △ damage, × fatal at full health. Hover for the lowest achievable damage."
)


INPUTS = (
    ("origin", "Jump position"),
    ("entry_shot", "Shoot portal 1 from"),
    ("entry", "Portal 1 position"),
    ("exit_shot", "Shoot portal 2 from"),
    ("exit", "Portal 2 position"),
)
OUTPUTS = (("entry", "Portal 1 reach"), ("landings", "Portal 2 landings"))


class PortalJumpOverlay:
    def __init__(self, window):
        self.window = window
        self.origin = self.entry = self.exit = None
        self.entry_shot = self.exit_shot = None
        self.active_map = None
        self.settings = self.surfaces = None
        self.error = None
        self.results = {}
        self.entries = {}
        self.statuses = {}
        self.preview = None
        self.clear_action = QAction("Clear Portal Jump", window)
        self.clear_action.triggered.connect(self.clear)
        self.controls_action = QWidgetAction(window)
        self.controls = QWidget()
        self.controls.setSizePolicy(QSizePolicy.Policy.Maximum, QSizePolicy.Policy.Fixed)
        layout = QHBoxLayout(self.controls)
        layout.setContentsMargins(8, 0, 0, 0)
        layout.setSpacing(6)
        layout.setAlignment(Qt.AlignmentFlag.AlignLeft)
        self.controls_action.setDefaultWidget(self.controls)
        self.input_selector = CompactComboBox()
        self.input_selector.setAccessibleName("Portal Jump input")
        self.output_selector = CompactComboBox()
        self.output_selector.setAccessibleName("Portal Jump output")
        for combo, choices, caption in (
            (self.input_selector, INPUTS, "Input"),
            (self.output_selector, OUTPUTS, "Output"),
        ):
            for key, label in choices:
                combo.addItem(label, key)
            combo.setSizeAdjustPolicy(QComboBox.SizeAdjustPolicy.AdjustToContents)
            combo.setSizePolicy(QSizePolicy.Policy.Maximum, QSizePolicy.Policy.Fixed)
            label = QLabel(caption)
            label.setBuddy(combo)
            label.setSizePolicy(QSizePolicy.Policy.Fixed, QSizePolicy.Policy.Fixed)
            layout.addWidget(label)
            layout.addWidget(combo)
            layout.addSpacing(8)
        self.takeoff = QComboBox()
        self.takeoff.addItems(["Step", "Jump"])
        self.takeoff.setAccessibleName("Portal takeoff")
        self.movement = QComboBox()
        self.movement.addItems(["Run", "Walk"])
        self.movement.setAccessibleName("Portal movement")
        self.margin = QDoubleSpinBox()
        self.margin.setDecimals(3)
        self.margin.setRange(0, 999.999)
        self.margin.setSingleStep(0.01)
        self.margin.setValue(0.1)
        self.margin.setSuffix(" s")
        self.margin.setKeyboardTracking(False)
        self.margin.setAccessibleName("Portal takeoff margin")
        self.margin.setToolTip("Reserve this much travel time before the takeoff edge; Jump only.")
        for widget in (self.takeoff, self.movement):
            widget.setSizePolicy(QSizePolicy.Policy.Maximum, QSizePolicy.Policy.Fixed)
            layout.addWidget(widget)
        layout.addSpacing(8)
        margin_label = QLabel("Margin")
        margin_label.setBuddy(self.margin)
        margin_label.setSizePolicy(QSizePolicy.Policy.Fixed, QSizePolicy.Policy.Fixed)
        layout.addWidget(margin_label)
        self.margin.setSizePolicy(QSizePolicy.Policy.Maximum, QSizePolicy.Policy.Fixed)
        layout.addWidget(self.margin)
        self.toolbar = QToolBar("Portal Jump", window)
        self.toolbar.setMovable(False)
        body = QWidget()
        layout = QHBoxLayout(body)
        layout.setContentsMargins(8, 0, 8, 0)
        self.legend = QLabel()
        self.legend.setTextFormat(Qt.TextFormat.RichText)
        self.legend.setToolTip(LIMITS)
        layout.addWidget(self.legend)
        self.use_jump_button = QPushButton("Use jump position")
        self.use_jump_button.setToolTip("Reset this shooting position to follow the jump position.")
        self.use_jump_button.clicked.connect(self.use_jump_position)
        layout.addWidget(self.use_jump_button)
        self.clear_button = QPushButton("Clear")
        self.clear_button.setAccessibleName("Clear Portal Jump")
        self.clear_button.clicked.connect(self.clear)
        layout.addWidget(self.clear_button)
        layout.addStretch()
        self.toolbar.addWidget(body)
        self.input_selector.currentIndexChanged.connect(self.selection_changed)
        self.output_selector.currentIndexChanged.connect(self.selection_changed)
        self.takeoff.currentIndexChanged.connect(self.recompute)
        self.movement.currentIndexChanged.connect(self.recompute)
        self.margin.valueChanged.connect(self.recompute)
        window.doc.changed.connect(self.document_changed)
        window.doc.replaced.connect(self.clear)
        self.reload_settings()

    def reload_settings(self):
        try:
            name = self.window.catalog_map
            self.settings = PortalSettings.from_settings(
                load_map_settings(name),
                str(map_settings_path(name)),
                gameplay=read_settings_json(constants.GAMEPLAY_PATH),
                gameplay_source=str(constants.GAMEPLAY_PATH),
            )
            self.error = None
        except (OSError, ValueError) as exc:
            self.settings = None
            self.error = str(exc)
        self.rebuild_surfaces()

    def rebuild_surfaces(self):
        self.surfaces = (
            PortalSurfaces(self.window.map_data, self.settings, self.window.texture_catalog) if self.settings else None
        )
        self.statuses = {}
        self.recompute()

    @property
    def has_selection(self):
        return any(getattr(self, key) is not None for key, _ in INPUTS)

    def firing_origin(self, end):
        position = self.entry_shot if end == "entry" else self.exit_shot
        return position if position is not None else self.origin

    def oriented(self, surface, end):
        origin = self.firing_origin(end)
        return surface.placed_from(origin) if origin is not None else surface

    def document_changed(self, before):
        if self.has_selection:
            after = self.window.map_data
            if self.active_map != self.window.doc.active_map or (
                before["grid_cols"],
                before["grid_rows"],
                len(before["levels"]),
            ) != (after["grid_cols"], after["grid_rows"], len(after["levels"])):
                self.clear()
        self.rebuild_surfaces()

    def clear(self):
        self.origin = self.entry = self.exit = None
        self.entry_shot = self.exit_shot = None
        self.active_map = None
        self.entries = {}
        self.results = {}
        self.selection_changed()

    def selection_changed(self):
        self.preview = None
        self.window.canvas._clear_hover()
        self.refresh()
        self.window.canvas.update()

    def use_jump_position(self):
        key = self.input_selector.currentData()
        if key in ("entry_shot", "exit_shot"):
            setattr(self, key, None)
            self.recompute()

    def status(self, surface):
        if surface not in self.statuses:
            self.statuses[surface] = self.surfaces.status(surface)
        return self.statuses[surface]

    def states(self, surface):
        if self.origin is None or self.settings is None:
            return {}
        if surface not in self.entries:
            self.entries[surface] = (
                entry_states(
                    self.settings,
                    self.origin,
                    surface,
                    self.window.map_data,
                    running=self.movement.currentText() == "Run",
                    jumping=self.takeoff.currentText() == "Jump",
                    margin=self.margin.value(),
                    footprints=self.surfaces.footprints,
                )
                if self.status(surface).available
                else {}
            )
        return self.entries[surface]

    def landing_issue(self):
        for key, label in (("origin", "jump position"), ("entry", "portal 1 position"), ("exit", "portal 2 position")):
            if getattr(self, key) is None:
                return f"Set {label}"
        for surface, name in ((self.entry, "Portal 1"), (self.exit, "Portal 2")):
            if not self.status(surface).available:
                return f"{name}: {self.status(surface).reason}"
        if portals_overlap(self.entry, self.exit, self.settings):
            return "Portal 1 and portal 2 overlap"
        if not self.states(self.entry):
            return "Portal 1 is out of reach from the jump position"
        return None

    def recompute(self):
        self.entries = {}
        self.results = {}
        self.margin.setEnabled(self.takeoff.currentText() == "Jump")
        if self.settings:
            for end in ("entry", "exit"):
                surface = getattr(self, end)
                if surface is not None:
                    setattr(self, end, self.oriented(surface, end))
            if self.landing_issue() is None:
                self.results = calculate_landings(
                    self.settings, self.entry, self.exit, self.states(self.entry), self.window.map_data
                )
        self.selection_changed()

    def refresh(self):
        active = self.has_selection
        selected = self.window.mode == constants.MODE_PORTAL_JUMP
        self.toolbar.setVisible(active or selected)
        self.controls_action.setVisible(selected)
        self.clear_action.setEnabled(active)
        self.clear_button.setVisible(active)
        key = self.input_selector.currentData()
        self.use_jump_button.setVisible(selected and key in ("entry_shot", "exit_shot"))
        self.use_jump_button.setEnabled(getattr(self, key) is not None)
        if self.error:
            self.legend.setText(f"Portal Jump unavailable: {escape(self.error)}")
            return
        colors = " &nbsp; ".join(f'<span style="color: {color}">{name}</span>' for _, name, color in SCENARIOS)
        if self.output_selector.currentData() == "entry":
            issue = "Set jump position" if self.origin is None else None
        else:
            issue = self.landing_issue()
        prompt = escape(issue or self.output_selector.currentText())
        edit = self.input_selector.currentText()
        if key in ("entry_shot", "exit_shot") and getattr(self, key) is None:
            edit += " (using jump position)"
        mode = f"{self.takeoff.currentText()} / {self.movement.currentText()}"
        self.legend.setText(
            f"Portal Jump · {prompt} &nbsp; {colors}<br>"
            f"{escape(edit)} &nbsp; {mode} &nbsp; Dashed surfaces: planned &nbsp; ● Safe &nbsp; △ Damage &nbsp; × Fatal"
        )

    def pick(self, point, end):
        if self.surfaces is None:
            return None
        surface = self.surfaces.pick(
            self.window.current_level, point.x(), point.y(), self.window.canvas.pick_tolerance()
        )
        return self.oriented(surface, end) if surface is not None else None

    def select(self, point):
        if self.settings is None:
            self.window.notify("Portal Jump unavailable: " + self.error)
            return
        key = self.input_selector.currentData()
        if key in ("origin", "entry_shot", "exit_shot"):
            col, row = int(point.x() // 1), int(point.y() // 1)
            if not (0 <= col < self.window.map_data["grid_cols"] and 0 <= row < self.window.map_data["grid_rows"]):
                return
            value = self.window.current_level, col, row
        else:
            value = self.pick(point, key)
            if value is None:
                return
            if not self.status(value).available:
                self.window.notify(self.status(value).reason)
                return
            other = self.exit if key == "entry" else self.entry
            if other is not None and portals_overlap(value, other, self.settings):
                self.window.notify("Portal 1 and portal 2 overlap")
                return
        setattr(self, key, value)
        self.active_map = self.window.doc.active_map
        self.recompute()

    def hover_text(self, point):
        key = self.input_selector.currentData()
        selected = self.window.mode == constants.MODE_PORTAL_JUMP
        preview = self.pick(point, key) if selected and key in ("entry", "exit") else None
        if preview != self.preview:
            self.preview = preview
            self.window.canvas.update()
        if self.settings is None or (not self.has_selection and not selected):
            return None
        lines = []
        cell = self.window.current_level, int(point.x() // 1), int(point.y() // 1)
        for position, label in (
            (self.origin, "J · Jump position"),
            (self.firing_origin("entry"), "S1 · Shoot portal 1 from"),
            (self.firing_origin("exit"), "S2 · Shoot portal 2 from"),
        ):
            if cell == position:
                lines.append(label)
        if preview is not None:
            lines.extend(
                [f"Set portal {1 if key == 'entry' else 2} · {preview.face.capitalize()}", self.status(preview).label]
            )
            other = self.exit if key == "entry" else self.entry
            if other is not None and portals_overlap(preview, other, self.settings):
                lines.append("Overlaps the other portal")
        elif selected:
            lines.append("Set " + self.input_selector.currentText().lower())
        if self.output_selector.currentData() == "entry":
            surface = self.pick(point, "entry")
            if surface is not None:
                lines.append("Portal Jump · Portal 1 reach")
                if self.origin is None:
                    lines.append("Set jump position")
                elif not self.status(surface).available:
                    lines.append(self.status(surface).reason)
                else:
                    states = self.states(surface)
                    lines.append(
                        "Reachable: " + ", ".join(name for bit, name, _ in SCENARIOS if bit in states)
                        if states
                        else "Out of reach"
                    )
        elif issue := self.landing_issue():
            lines.append("Portal Jump · " + issue)
        else:
            landings = self.results.get(cell, {})
            existing = cell[1:] in self.surfaces.floors[cell[0]]
            lines.append("Portal Jump · " + ("Existing landing floor" if existing else "Planned landing floor"))
            lines.extend(landing_lines(landings) if landings else ["Out of range"])
        return "\n".join(lines) or None

    def paint_surface(self, painter, cell, surface, color, label="", *, planned=False, markers=None):
        frame = surface.frame(self.settings)
        scale = cell / self.settings.movement.cell_size
        x, z = frame.center[0] * scale, frame.center[2] * scale
        direction = frame.up if surface.face == "floor" else frame.normal
        painter.setPen(
            QPen(QColor(color), 2 if label else 1, Qt.PenStyle.DashLine if planned else Qt.PenStyle.SolidLine)
        )
        painter.setBrush(Qt.BrushStyle.NoBrush)
        if surface.face == "floor":
            rx, rz = (
                (PORTAL_HALF_WIDTH, PORTAL_HALF_HEIGHT)
                if surface.turn % 2 == 0
                else (PORTAL_HALF_HEIGHT, PORTAL_HALF_WIDTH)
            )
            painter.drawEllipse(QRectF(x - rx * scale, z - rz * scale, 2 * rx * scale, 2 * rz * scale))
        else:
            dx, dz = frame.right[0] * cell * 0.23, frame.right[2] * cell * 0.23
            painter.drawLine(QPointF(x - dx, z - dz), QPointF(x + dx, z + dz))
        length = cell * (0.28 if label else 0.16)
        tip = QPointF(x + direction[0] * length, z + direction[2] * length)
        painter.drawLine(QPointF(x, z), tip)
        for sign in (-1, 1):
            painter.drawLine(
                tip,
                tip - QPointF(direction[0] - sign * direction[2], direction[2] + sign * direction[0]) * length * 0.3,
            )
        if label:
            badge = QRectF(x + cell * 0.13, z - cell * 0.3, max(14, cell * 0.2), max(14, cell * 0.2))
            painter.setBrush(QColor("#111418"))
            painter.drawRoundedRect(badge, 3, 3)
            painter.drawText(badge, Qt.AlignmentFlag.AlignCenter, label)
        if markers:
            radius = min(2.5, cell * 0.06)
            horizontal = surface.face in ("floor", "north", "south")
            for index, (bit, _, shade) in enumerate(SCENARIOS):
                if bit not in markers:
                    continue
                offset = (index - 1.5) * cell * 0.14
                px = x + (offset if horizontal else frame.normal[0] * cell * 0.15)
                pz = z + (frame.normal[2] * cell * 0.15 if horizontal else offset)
                if surface.face == "floor":
                    pz += cell * 0.32
                painter.setPen(Qt.PenStyle.NoPen)
                painter.setBrush(QColor(shade))
                painter.drawEllipse(QPointF(px, pz), radius, radius)

    def paint(self, painter, cell):
        if self.settings is None or (not self.has_selection and self.preview is None):
            return
        level = self.window.current_level
        canvas = self.window.canvas
        visible = canvas.viewport.visible_rect(canvas.width(), canvas.height()).adjusted(-1, -1, 1, 1)
        painter.save()
        if self.output_selector.currentData() == "entry" and self.origin is not None:
            for surface in self.surfaces.candidates(level):
                if not visible.contains(QPointF(surface.col, surface.row)):
                    continue
                surface = self.oriented(surface, "entry")
                status = self.status(surface)
                if not status.available:
                    continue
                states = self.states(surface)
                if states:
                    self.paint_surface(painter, cell, surface, "#7a9caa", planned=status.planned, markers=states)
        elif self.output_selector.currentData() == "landings":
            for row in range(
                max(0, floor(visible.top())), min(self.window.map_data["grid_rows"], ceil(visible.bottom()))
            ):
                for col in range(
                    max(0, floor(visible.left())), min(self.window.map_data["grid_cols"], ceil(visible.right()))
                ):
                    landings = self.results.get((level, col, row), {})
                    if landings:
                        if (col, row) not in self.surfaces.floors[level]:
                            painter.setPen(QPen(QColor("#7a9caa"), 1, Qt.PenStyle.DashLine))
                            painter.setBrush(Qt.BrushStyle.NoBrush)
                            painter.drawRect(QRectF((col + 0.08) * cell, (row + 0.08) * cell, cell * 0.84, cell * 0.84))
                        paint_landing_markers(painter, cell, col, row, landings)
        self.paint_positions(painter, cell)
        for surface, color, label in (
            (self.entry, "#60a5fa", "1"),
            (self.exit, "#fb923c", "2"),
            (self.preview, "#ffffff", ""),
        ):
            if surface is not None and surface.level == level:
                status = self.status(surface)
                self.paint_surface(
                    painter, cell, surface, color if status.available else "#ef4444", label, planned=status.planned
                )
        painter.restore()

    def paint_positions(self, painter, cell):
        positions = {}
        for position, label, color in (
            (self.origin, "J", "#ffffff"),
            (self.firing_origin("entry"), "S1", "#60a5fa"),
            (self.firing_origin("exit"), "S2", "#fb923c"),
        ):
            if position is not None and position[0] == self.window.current_level:
                positions.setdefault(position, []).append((label, color))
        for (_, col, row), labels in positions.items():
            painter.setPen(QPen(QColor(labels[0][1]), 2, Qt.PenStyle.DashLine))
            painter.setBrush(Qt.BrushStyle.NoBrush)
            rect = QRectF((col + 0.04) * cell, (row + 0.04) * cell, cell * 0.92, cell * 0.92)
            painter.drawRect(rect)
            for index, (label, color) in enumerate(labels):
                painter.setPen(QColor(color))
                painter.drawText(
                    QRectF(rect.x() + index * cell * 0.3, rect.y(), cell * 0.3, rect.height()),
                    Qt.AlignmentFlag.AlignTop | Qt.AlignmentFlag.AlignLeft,
                    label,
                )
