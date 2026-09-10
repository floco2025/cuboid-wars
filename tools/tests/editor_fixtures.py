"""Fixtures for the editor tests: map record helpers, a widget-free host
for the editing mixins, and a shown editor window on a small map."""

import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from PySide6.QtCore import QEvent, QPoint, QSettings, Qt
from PySide6.QtTest import QTest
from PySide6.QtWidgets import QApplication

from map_editor.constants import FACES
from map_editor.erase_tools import EraseMixin
from map_editor.io import write_map
from map_editor.items import ItemsMixin
from map_editor.lights import LightsMixin
from map_editor.nested_maps import NestedMapsMixin
from map_editor.nesting import NestedMapShape
from map_editor.normalization import empty_map
from map_editor.placement import PlacementMixin
from map_editor.select import SelectMixin
from map_editor.spawn_zones import SpawnZoneEditMixin
from map_editor.window import EditorWindow


DEFAULT_ALIAS = "basement-floor"


def qt_app() -> QApplication:
    return QApplication.instance() or QApplication([])


def faces(alias: str = DEFAULT_ALIAS) -> dict[str, str]:
    return {face: alias for face in FACES}


def floor(col: int, row: int) -> dict:
    return {"col": col, "row": row, **faces()}


def nested(map_name: str, level: int, start: list[int], end: list[int], to_level: int | None = None) -> dict:
    return {
        "map": map_name,
        "level": level,
        "from": start,
        "to": end,
        "to_level": level if to_level is None else to_level,
        "travel_secs": 2.0,
        "pause_secs": 1.0,
        "phase_secs": 0.0,
        "from_nudge": [0.0, 0.0, 0.0],
        "to_nudge": [0.0, 0.0, 0.0],
    }


# An 8x8 map whose cell (1, 1) holds a floor, a north wall with a light,
# a plate on the `barrier_1` switch, and a gold item, and (2, 2) a bare floor.
def furnished_map() -> dict:
    data = empty_map(8, 8)
    level = data["levels"][0]
    level["floors"] = [floor(1, 1), floor(2, 2)]
    level["walls"] = [{"c0": 1, "r0": 1, "c1": 2, "r1": 1, "all": DEFAULT_ALIAS}]
    level["lights"] = [{"col": 1, "row": 1, "side": "N"}]
    data["pressure_plates"] = [{"level": 0, "col": 1, "row": 1, "switch": "barrier_1"}]
    data["items"] = [{"level": 0, "col": 1, "row": 1, "type": "gold"}]
    data["player_spawn_zones"] = []
    return data


# Stand-in geometry for nested-map tests: `cabin` is a 3x2 room on two
# storeys, `loop_a` and `loop_b` nest each other.
NESTED_SHAPES = {
    "cabin": NestedMapShape(grid_cols=3, grid_rows=2, level_count=2, nested_names=()),
    "loop_a": NestedMapShape(grid_cols=1, grid_rows=1, level_count=1, nested_names=("loop_b",)),
    "loop_b": NestedMapShape(grid_cols=1, grid_rows=1, level_count=1, nested_names=("loop_a",)),
}


class StubCanvas:
    def update(self) -> None:
        pass

    def cells_per_pixel(self, pixels: float) -> float:
        return pixels / 36.0

    def pick_tolerance(self) -> float:
        return self.cells_per_pixel(6.0)


class EditorHost(PlacementMixin, ItemsMixin, LightsMixin, NestedMapsMixin, EraseMixin, SelectMixin, SpawnZoneEditMixin):
    """The editing mixins over a plain map, or over a `MapDocument` when a
    test needs the undo history of its edits."""

    def __init__(self, map_data: dict | None, bridge_kinds: list[str], doc=None) -> None:
        self.doc = doc
        self._map_data = map_data
        self.current_level = 0
        self.bridge_kinds = bridge_kinds
        self.barrier_kinds = ["barrier_1"]
        self.switches = ["barrier_1", "fireworks"]
        self.recent_pressure_plate_switch = "barrier_1"
        self.recent_actor_spawn_switch = ""
        self.canvas = StubCanvas()
        self.spawn_zone_drag = None
        self.current_material = DEFAULT_ALIAS
        self.selected_spawn_zone_ref = None
        self.tile_selection = None
        self.select_drag_kind = None
        self.statuses: list[str] = []
        self.path = None
        self.recent_nested_map = None
        self.recent_light_kind = "decorative"

    @property
    def map_data(self) -> dict:
        return self._map_data if self.doc is None else self.doc.map_data

    def edited_map_name(self):
        return self.path.stem if self.path else None

    def nested_map_shape(self, name: str) -> NestedMapShape | None:
        return NESTED_SHAPES.get(name)

    def apply_change(self, label: str, after: dict) -> None:
        if self.doc is None:
            self._map_data = after
        else:
            self.doc.apply_change(label, after)

    def notify(self, message: str) -> None:
        self.statuses.append(message)

    def update_selection_actions(self) -> None:
        pass


class WindowTestCase(unittest.TestCase):
    """An 8x8 map with one floor at (1, 1) and no spawn zones, open in a
    shown window whose autosave timer is stopped."""

    @classmethod
    def setUpClass(cls):
        cls.app = qt_app()

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.path = Path(self.temp.name) / "hotel" / "layout.json"
        data = empty_map(8, 8)
        data["player_spawn_zones"] = []
        data["levels"][0]["floors"] = [{"col": 1, "row": 1, "all": DEFAULT_ALIAS}]
        write_map(self.path, data)
        self.recents = patch.object(EditorWindow, "_record_recent_path")
        self.recents.start()
        self.app.clipboard().clear()
        preferences = QSettings(str(Path(self.temp.name) / "preferences.ini"), QSettings.Format.IniFormat)
        self.window = EditorWindow(self.path, preferences=preferences)
        self.window._autosave_timer.stop()
        self.window.show()
        self.window.activateWindow()
        self.app.processEvents()

    def tearDown(self):
        with patch.object(self.window, "confirm_discard_changes", return_value=True):
            self.window.close()
        self.window.deleteLater()
        self.app.sendPostedEvents(None, QEvent.Type.DeferredDelete)
        # A `QMimeData` left on the clipboard is freed by Qt's global holder
        # after the interpreter tore down its Python wrapper, which crashes
        # the process on exit.
        self.app.clipboard().clear()
        self.recents.stop()
        self.temp.cleanup()

    def click(self, col, row):
        size = self.window.canvas.cell_size()
        QTest.mouseClick(self.window.canvas, Qt.MouseButton.LeftButton, pos=QPoint(round((col + .5) * size), round((row + .5) * size)))
