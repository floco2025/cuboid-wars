import tempfile
import unittest
from pathlib import Path

from PySide6.QtCore import QByteArray, QEvent, QSettings, QSize
from PySide6.QtTest import QTest
from PySide6.QtWidgets import QApplication, QMainWindow

from map_editor.window_geometry import WindowGeometry


class WindowGeometryTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.app = QApplication.instance() or QApplication([])

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.settings_path = str(Path(self.temp.name) / "preferences.ini")
        self.preferences = QSettings(self.settings_path, QSettings.Format.IniFormat)
        self.windows = []

    def tearDown(self):
        for window in self.windows:
            window.close()
            window.deleteLater()
        self.app.sendPostedEvents(None, QEvent.Type.DeferredDelete)
        self.temp.cleanup()

    def make_window(self):
        window = QMainWindow()
        window.window_geometry = WindowGeometry(window, self.preferences)
        self.windows.append(window)
        return window

    def show_normal(self):
        window = self.make_window()
        window.resize(600, 450)
        window.move(60, 70)
        window.show()
        self.app.processEvents()
        return window

    def test_missing_or_invalid_preferences_use_default_size(self):
        for value in (None, "invalid", 42, QByteArray(b"invalid")):
            with self.subTest(value=value):
                self.preferences.setValue(WindowGeometry.KEY, value)
                window = self.make_window()
                self.assertEqual(window.size(), QSize(1000, 800))
                self.assertFalse(window.isMaximized())

    def test_size_and_position_round_trip(self):
        original = self.show_normal()
        original.window_geometry.save()
        restored = self.make_window()
        restored.show()
        self.app.processEvents()
        self.assertEqual(restored.geometry(), original.geometry())
        self.assertEqual(restored.pos(), original.pos())

    def test_maximized_state_preserves_normal_size_and_position(self):
        original = self.show_normal()
        normal = original.geometry()
        original.showMaximized()
        self.app.processEvents()
        original.window_geometry.save()
        restored = self.make_window()
        restored.show()
        self.app.processEvents()
        self.assertTrue(restored.isMaximized())
        self.assertEqual(restored.normalGeometry(), normal)
        restored.showNormal()
        self.app.processEvents()
        self.assertEqual(restored.geometry(), normal)

    def test_minimized_window_reopens_visible_with_its_prior_state(self):
        for maximized in (False, True):
            with self.subTest(maximized=maximized):
                original = self.show_normal()
                if maximized:
                    original.showMaximized()
                original.showMinimized()
                self.app.processEvents()
                original.window_geometry.save()
                restored = self.make_window()
                restored.show()
                self.app.processEvents()
                self.assertFalse(restored.isMinimized())
                self.assertEqual(restored.isMaximized(), maximized)
                self.assertEqual(restored.normalGeometry(), original.normalGeometry())

    def test_offscreen_position_is_restored_on_screen(self):
        original = self.show_normal()
        original.move(100000, 100000)
        self.app.processEvents()
        original.window_geometry.save()
        restored = self.make_window()
        restored.show()
        self.app.processEvents()
        self.assertTrue(restored.screen().availableGeometry().contains(restored.geometry()))

    def test_changes_are_saved_after_a_short_pause_without_closing(self):
        window = self.show_normal()
        window.window_geometry.timer.setInterval(10)
        for change in (
            lambda: window.resize(620, 470),
            lambda: window.move(80, 90),
            window.showMaximized,
        ):
            with self.subTest(change=change):
                change()
                self.app.processEvents()
                self.assertTrue(window.window_geometry.timer.isActive())
                QTest.qWait(30)
                saved = QSettings(self.settings_path, QSettings.Format.IniFormat)
                self.assertEqual(saved.value(WindowGeometry.KEY), window.saveGeometry())
                self.assertFalse(window.window_geometry.timer.isActive())
