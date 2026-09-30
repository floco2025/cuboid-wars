"""The Jump Path tool in the window: its inputs, the two previews it keeps, legend, and hover."""

import json
from html import escape
from math import hypot

from PySide6.QtCore import Qt
from PySide6.QtGui import QAction
from PySide6.QtWidgets import (
    QCheckBox,
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

from .catalogs import load_map_settings, map_settings_path
from .compact_widgets import CompactComboBox
from .constants import MODE_JUMP_PATH
from .floor_footprints import FloorFootprints
from .jump_path import (
    AFTER_EXIT,
    Takeoff,
    build_request,
    entry_outcome,
    jump_preview,
    level_regions,
    level_view,
    takeoff_from_click,
)
from .jump_path_painting import paint_jump_path
from .jump_settings import ANTI_GRAVITY, BOTH, NORMAL, SPEED, JumpSettings
from .portal_surfaces import PortalSurfaces, portals_overlap
from .reach_markers import landing_line

LIMITS = (
    "Flights run the game's own movement, portal funnel, and portal crossing per server tick, in open space. "
    "The takeoff is the clicked point on a tile edge, heading straight out at full speed: Step walks off it, "
    "Jump leaves the ground the margin before it. Input is released in the air. "
    "A path is thick down to the floor of the level in view and thin once it has fallen past it. "
    "Landings show where the flight comes down on every level: ● safe, △ damage, × fatal at full health; "
    "hollow where no floor is under it, faded where a floor above catches the flight first. "
    "The solid outline holds the floor-portal centres that take the flight in, as portal 1 would be shot: "
    "a portal draws a nearby flight in without changing its velocity, so it leaves portal 2 at the angle it "
    "went into portal 1. "
    "Air control adds the landings steering can reach (dotted) and the centres it can then enter (dashed), "
    "and a missed portal 1 is tried again steered at it. "
    "Floor portals may be centred anywhere and face the way their shooter (S1, S2) faces, initially the takeoff; "
    "wall portals sit on a tile edge with their rim on the wall's base. "
    "Ignores obstacles, ramps, light bridges, moving platforms, placement nudging and backing size. "
    "The selected power-ups last throughout. Dashed portals are on planned surfaces."
)

INPUTS = (
    ("takeoff", "Takeoff"),
    ("entry_floor", "Portal 1 floor"),
    ("entry_wall", "Portal 1 wall"),
    ("exit_floor", "Portal 2 floor"),
    ("exit_wall", "Portal 2 wall"),
    ("entry_shot", "Shooter 1"),
    ("exit_shot", "Shooter 2"),
)
# What a portal input places: which portal, and whether on a wall.
PORTAL_INPUTS = {
    "entry_floor": ("entry", False),
    "entry_wall": ("entry", True),
    "exit_floor": ("exit", False),
    "exit_wall": ("exit", True),
}
SHOTS = {"entry": "entry_shot", "exit": "exit_shot"}
# Everything a click can set.
PLACED = ("takeoff", "entry", "exit", "entry_shot", "exit_shot")
POWER_UPS = (
    (NORMAL, "No power-ups"),
    (SPEED, "Speed"),
    (ANTI_GRAVITY, "Anti-gravity"),
    (BOTH, "Speed + anti-gravity"),
)
ENTRY_LABELS = {
    "direct": "enters",
    "funnel": "drawn in",
    "steered": "steered in",
    "reversed": "meets portal 2 first",
}
GLYPH_PICK_PIXELS = 9
# Extra space between the toolbar controls, on top of the row's own spacing.
GROUP_GAP = 8


class JumpPathOverlay:
    def __init__(self, window):
        self.window = window
        self.takeoff = self.entry = self.exit = None
        # Overrides of the takeoff as the place a portal is shot from: (level, x, z) in grid units.
        self.entry_shot = self.exit_shot = None
        self.active_map = None
        self.settings = self.surfaces = self.footprints = None
        self.error = None
        # Every power-up combination's flight with no portals, and through the placed pair.
        self.free = self.through = None
        self.issue = None
        self.preview = None
        self.statuses = {}
        self._replies = {}
        self._views = {}
        self.clear_action = QAction("Clear Jump Path", window)
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
        self.input_selector.setAccessibleName("Jump Path input")
        self.input_selector.setToolTip(
            "What the next click on the canvas sets. A floor portal is centred on the click; a wall portal takes "
            "the nearest grid edge. A shooter is where its portal is shot from, the takeoff until set."
        )
        for key, label in INPUTS:
            self.input_selector.addItem(label, key)
        self.input_selector.setSizeAdjustPolicy(QComboBox.SizeAdjustPolicy.AdjustToContents)
        self.input_selector.setMinimumContentsLength(0)
        self.input_selector.setSizePolicy(QSizePolicy.Policy.Maximum, QSizePolicy.Policy.Fixed)
        layout.addWidget(self.input_selector)
        self.kind = QComboBox()
        self.kind.addItems(["Jump", "Step"])
        self.kind.setAccessibleName("Jump Path takeoff")
        self.kind.setToolTip("Jump off the edge, or walk off it.")
        self.air_control = QCheckBox("Air control")
        self.air_control.setAccessibleName("Jump Path air control")
        self.air_control.setToolTip(
            "Off: input is released in the air, so the map's air braking applies. "
            "On: also show where steering can land and which portal centres it can then enter."
        )
        self.margin = QDoubleSpinBox()
        self.margin.setDecimals(3)
        self.margin.setRange(0, 999.999)
        self.margin.setSingleStep(0.01)
        self.margin.setValue(0.1)
        self.margin.setSuffix(" s")
        self.margin.setKeyboardTracking(False)
        self.margin.setAccessibleName("Jump Path takeoff margin")
        self.margin.setToolTip("Takeoff margin: jump this much travel time before the edge. Jump only.")
        # The margin sits against Jump, which it belongs to.
        for widget, gap in ((self.kind, GROUP_GAP), (self.margin, 0), (self.air_control, GROUP_GAP)):
            widget.setSizePolicy(QSizePolicy.Policy.Maximum, QSizePolicy.Policy.Fixed)
            layout.addSpacing(gap)
            layout.addWidget(widget)
        self.toolbar = QToolBar("Jump Path", window)
        self.toolbar.setMovable(False)
        # The row's controls come first and keep their size; the legend takes
        # what is left, so long text is cut off instead of pushing them out.
        body = QWidget()
        body.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Preferred)
        layout = QHBoxLayout(body)
        layout.setContentsMargins(8, 0, 8, 0)
        self.power_ups = QComboBox()
        self.power_ups.setAccessibleName("Jump Path power-ups")
        self.power_ups.setToolTip("The power-ups the player holds for the whole flight.")
        for bit, label in POWER_UPS:
            self.power_ups.addItem(label, bit)
        self.power_ups.setSizePolicy(QSizePolicy.Policy.Fixed, QSizePolicy.Policy.Fixed)
        layout.addWidget(self.power_ups)
        self.clear_button = QPushButton("Clear")
        self.clear_button.setAccessibleName("Clear Jump Path")
        self.clear_button.clicked.connect(self.clear)
        layout.addWidget(self.clear_button)
        self.use_takeoff_button = QPushButton("Use takeoff")
        self.use_takeoff_button.setToolTip("Shoot this shooter's portal from the takeoff again.")
        self.use_takeoff_button.clicked.connect(self.use_takeoff)
        layout.addWidget(self.use_takeoff_button)
        layout.addSpacing(GROUP_GAP)
        self.legend = QLabel()
        self.legend.setTextFormat(Qt.TextFormat.RichText)
        self.legend.setToolTip(LIMITS)
        self.legend.setSizePolicy(QSizePolicy.Policy.Ignored, QSizePolicy.Policy.Preferred)
        layout.addWidget(self.legend, 1)
        self.toolbar.addWidget(body)
        self.input_selector.currentIndexChanged.connect(self.selection_changed)
        self.power_ups.currentIndexChanged.connect(self.power_ups_changed)
        self.kind.currentIndexChanged.connect(self.recompute)
        self.air_control.toggled.connect(self.recompute)
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
        self._replies = {}
        self.rebuild()

    # Everything read from the document: surfaces, floor support, and the takeoff's edge.
    def rebuild(self):
        data = self.window.map_data
        if self.settings is None:
            self.surfaces = self.footprints = None
        else:
            self.surfaces = PortalSurfaces(data, self.window.texture_catalog)
            self.footprints = FloorFootprints(data, self.settings.cell_size, self.settings.wall_thickness)
        self.statuses = {}
        self.recompute()

    @property
    def has_selection(self):
        return any(getattr(self, key) is not None for key in PLACED)

    @property
    def jumping(self):
        return self.kind.currentText() == "Jump"

    # Where a portal is shot from, in grid units: its override, else the takeoff.
    def shooter(self, end):
        override = getattr(self, SHOTS[end])
        if override is not None:
            return override[1:]
        return self.takeoff.grid_point if self.takeoff is not None else None

    def oriented(self, surface, end):
        shooter = self.shooter(end)
        return surface.placed_from(shooter) if shooter is not None else surface

    def document_changed(self, before):
        if self.has_selection:
            after = self.window.map_data
            if self.active_map != self.window.doc.active_map or (
                before["grid_cols"],
                before["grid_rows"],
                len(before["levels"]),
            ) != (after["grid_cols"], after["grid_rows"], len(after["levels"])):
                self.clear()
                return
        self.rebuild()

    def clear(self):
        self.takeoff = self.entry = self.exit = None
        self.entry_shot = self.exit_shot = None
        self.active_map = None
        self.rebuild()

    def selection_changed(self):
        self.window.canvas._clear_hover()
        self.refresh()
        self.window.canvas.update()

    def power_ups_changed(self):
        self._views = {}
        self.selection_changed()

    def use_takeoff(self):
        key = self.input_selector.currentData()
        if key in SHOTS.values():
            setattr(self, key, None)
            self.recompute()

    def status(self, surface):
        if surface not in self.statuses:
            self.statuses[surface] = self.surfaces.status(surface)
        return self.statuses[surface]

    # Why the placed portals cannot be flown through; the inputs stay for correction.
    def portal_issue(self):
        for surface, name in ((self.entry, "Portal 1"), (self.exit, "Portal 2")):
            if surface is not None and not self.status(surface).available:
                return f"{name}: {self.status(surface).reason}"
        if self.entry is None:
            return "Set portal 1" if self.exit is not None else None
        if self.exit is not None and portals_overlap(self.entry, self.exit, self.settings):
            return "Portal 1 and portal 2 overlap"
        return None

    # An unchanged request keeps its reply under the same settings, so edits
    # that move nothing the flight reads, a floor under a landing for one,
    # need no new preview.
    def fly(self, replies, entry=None, exit=None):
        request = build_request(
            self.settings,
            self.footprints,
            self.takeoff,
            levels=len(self.window.map_data["levels"]),
            jumping=self.jumping,
            margin=self.margin.value(),
            air_control=self.air_control.isChecked(),
            shooter=self.shooter("entry"),
            entry=entry,
            exit=exit,
        )
        key = json.dumps(request, sort_keys=True)
        replies[key] = self._replies[key] if key in self._replies else jump_preview(self.settings, request)
        return replies[key]

    def recompute(self):
        self.free = self.through = None
        self.issue = None
        self._views = {}
        self.margin.setEnabled(self.jumping)
        replies = {}
        if self.settings is not None:
            for end in SHOTS:
                surface = getattr(self, end)
                if surface is not None:
                    setattr(self, end, self.oriented(surface, end))
            self.issue = self.portal_issue()
            if self.takeoff is not None:
                self.free = self.fly(replies)
                if self.entry is not None and self.issue is None:
                    self.through = self.fly(replies, self.entry, self.exit)
        self._replies = replies
        self.selection_changed()

    def selected(self, flights):
        return next(flight for flight in flights if flight.bit == self.power_ups.currentData())

    # The flight drawn: through the portals once portal 1 can be flown into.
    @property
    def flight(self):
        flights = self.through if self.through is not None else self.free
        return self.selected(flights) if flights is not None else None

    def view(self, level):
        if self.flight is None:
            return None
        if level not in self._views:
            self._views[level] = level_view(self.flight, level, self.settings, self.footprints)
        return self._views[level]

    # Capture regions belong to the free flight: shown until portal 1 is placed, and again while it is being moved.
    def regions(self, level):
        if self.free is None:
            return []
        apex = tuple(value * self.settings.cell_size for value in self.shooter("entry"))
        free = level_regions(self.selected(self.free), level, apex)
        if self.through is None:
            return free
        placing = self.window.mode == MODE_JUMP_PATH and self.input_selector.currentData() in (
            "entry_floor",
            "entry_wall",
        )
        return level_regions(self.selected(self.through), level, apex) + (free if placing else [])

    # The portal a flight that falls back in goes into, and its number.
    def reentry(self):
        flight = self.flight
        if flight is None or flight.end != "reentered":
            return None
        x, y, z = flight.path[-1]

        def distance(portal):
            cx, cy, cz = portal[0].frame(self.settings).center
            return hypot(cx - x, cy - y, cz - z)

        return min(((self.entry, 1), (self.exit, 2)), key=distance)

    def entry_outcome(self):
        return entry_outcome(self.selected(self.through), self.footprints) if self.through is not None else None

    def prompt(self):
        if self.takeoff is None:
            return "Click near a tile edge to set the takeoff"
        if self.issue:
            return self.issue
        if self.entry is None:
            return "Landings by level; the outline catches a floor portal"
        outcome = self.entry_outcome()
        if outcome == "missed":
            return "Misses portal 1: showing the flight without it"
        if outcome == "blocked":
            return "A floor catches the flight before portal 1"
        flown = "Into portal 1" if self.exit is None else "Through portals 1 and 2"
        text = f"{flown}: {ENTRY_LABELS[outcome]}"
        # Out of a floor portal with too little sideways speed, the body comes back down into it.
        if self.flight.end == "reentered":
            text += f", then falls back into portal {self.reentry()[1]}; steering lands in the dotted outline"
        return text

    def refresh(self):
        active = self.has_selection
        selected = self.window.mode == MODE_JUMP_PATH
        self.toolbar.setVisible(active or selected)
        self.controls_action.setVisible(selected)
        self.clear_action.setEnabled(active)
        self.clear_button.setVisible(active)
        key = self.input_selector.currentData()
        shot = key in SHOTS.values()
        self.use_takeoff_button.setVisible(selected and shot)
        self.use_takeoff_button.setEnabled(shot and getattr(self, key) is not None)
        if self.error:
            self.legend.setText(f"Jump Path unavailable: {escape(self.error)}")
            return
        edit = self.input_selector.currentText()
        if shot and getattr(self, key) is None:
            edit += " (the takeoff)"
        mode = f"{self.kind.currentText()} / Air control {'on' if self.air_control.isChecked() else 'off'}"
        self.legend.setText(
            f"Jump Path · {escape(self.prompt())}<br>"
            f"{escape(edit)} &nbsp; {mode} &nbsp; ● Safe &nbsp; △ Damage &nbsp; × Fatal &nbsp; Hollow: no floor"
        )

    # The surface a portal input would place at a point in grid units.
    def pick(self, point, key):
        if self.surfaces is None:
            return None
        end, on_wall = PORTAL_INPUTS[key]
        pick = self.surfaces.pick_wall if on_wall else self.surfaces.pick_floor
        surface = pick(self.window.current_level, point.x(), point.y())
        return self.oriented(surface, end) if surface is not None else None

    def pick_takeoff(self, point):
        if self.footprints is None:
            return None
        data = self.window.map_data
        return takeoff_from_click(
            self.window.current_level, point.x(), point.y(), data["grid_cols"], data["grid_rows"], self.footprints.cells
        )

    def select(self, point):
        if self.settings is None:
            self.window.notify("Jump Path unavailable: " + self.error)
            return
        key = self.input_selector.currentData()
        if key == "takeoff":
            value = self.pick_takeoff(point)
            if value is None:
                return
        elif key in PORTAL_INPUTS:
            value = self.pick(point, key)
            if value is None:
                return
            if not self.status(value).available:
                self.window.notify(self.status(value).reason)
                return
            key = PORTAL_INPUTS[key][0]
            other = self.exit if key == "entry" else self.entry
            if other is not None and portals_overlap(value, other, self.settings):
                self.window.notify("Portal 1 and portal 2 overlap")
                return
        else:
            if self.window.canvas.input.cell(point) is None:
                return
            value = self.window.current_level, point.x(), point.y()
        setattr(self, key, value)
        self.active_map = self.window.doc.active_map
        self.recompute()

    # What a click here would set, for the inputs that are drawn before they are set.
    def pick_preview(self, point):
        if self.window.mode != MODE_JUMP_PATH or self.settings is None:
            return None
        key = self.input_selector.currentData()
        if key == "takeoff":
            return self.pick_takeoff(point)
        return self.pick(point, key) if key in PORTAL_INPUTS else None

    def glyphs_at(self, point):
        view = self.view(self.window.current_level)
        if view is None:
            return []
        reach = self.window.canvas.cells_per_pixel(GLYPH_PICK_PIXELS)
        size = self.settings.cell_size
        return [
            glyph
            for glyph in view.glyphs
            if hypot(glyph.point[0] / size - point.x(), glyph.point[1] / size - point.y()) <= reach
        ]

    # Tracks the pointer for the input preview. True when it changed.
    def hover(self, point):
        preview = self.pick_preview(point)
        changed = preview != self.preview
        self.preview = preview
        return changed

    def clear_hover(self):
        changed = self.preview is not None
        self.preview = None
        return changed

    def hover_text(self, point):
        selected = self.window.mode == MODE_JUMP_PATH
        if self.settings is None or not (self.has_selection or selected):
            return None
        lines = []
        preview = self.pick_preview(point)
        key = self.input_selector.currentData()
        if isinstance(preview, Takeoff):
            lines.append("Set takeoff")
        elif preview is not None:
            end = PORTAL_INPUTS[key][0]
            lines.extend(
                [f"Set portal {1 if end == 'entry' else 2} · {preview.face.capitalize()}", self.status(preview).label]
            )
            other = self.exit if end == "entry" else self.entry
            if other is not None and portals_overlap(preview, other, self.settings):
                lines.append("Overlaps the other portal")
        elif selected and key in SHOTS.values() and self.window.canvas.input.cell(point) is not None:
            lines.append("Set " + self.input_selector.currentText().lower())
        for glyph in self.glyphs_at(point):
            lines.append("Jump Path landing after portal 2" if glyph.phase == AFTER_EXIT else "Jump Path landing")
            lines.append(landing_line(glyph.damage))
            if not glyph.supported:
                lines.append("No floor here")
            if glyph.blocked:
                lines.append("A floor catches the flight earlier")
        return "\n".join(lines) or None

    def paint(self, painter, cell):
        paint_jump_path(self, painter, cell)
