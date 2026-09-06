"""A shown editor window on a small map, for the window tests."""

import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from PySide6.QtCore import QEvent, QPoint, QSettings, Qt
from PySide6.QtTest import QTest
from PySide6.QtWidgets import QApplication

from map_editor.constants import DEFAULT_ALIAS
from map_editor.io import empty_map, write_map
from map_editor.window import EditorWindow


class WindowTestCase(unittest.TestCase):
    """An 8x8 map with one floor at (1, 1) and no spawn zones, open in a
    shown window whose autosave timer is stopped."""

    @classmethod
    def setUpClass(cls):
        cls.app = QApplication.instance() or QApplication([])

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.path = Path(self.temp.name) / "map.json"
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
