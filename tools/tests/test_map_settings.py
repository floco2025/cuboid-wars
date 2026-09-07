import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from PySide6.QtTest import QTest

from editor_fixtures import WindowTestCase
from map_editor.constants import (
    GAMEPLAY_PATH,
    list_map_names,
    load_map_barrier_kinds,
    load_map_settings,
    map_layout_path,
    map_name_from_path,
    map_settings_path,
)
from map_editor.io import read_map


class MapSettingsTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.global_path = self.root / "gameplay.json"
        self.global_path.write_text(json.dumps({"default_map": "hotel", "maps": ["hotel"]}))
        for target, value in [
            ("map_editor.constants.GAMEPLAY_PATH", self.global_path),
            ("map_editor.constants.MAPS_DIR", self.root / "maps"),
        ]:
            override = patch(target, value)
            override.start()
            self.addCleanup(override.stop)

    def test_registry_rejects_invalid_names_duplicates_and_default(self):
        for names, default_map in [([], "hotel"), ([""], ""), (["../hotel"], "../hotel"),
                                   (["hotel", "hotel"], "hotel"), ({"hotel": {}}, "hotel"),
                                   ([1], "hotel"), (["hotel"], "missing"), (["hotel"], [])]:
            with self.subTest(names=names, default_map=default_map):
                self.global_path.write_text(json.dumps({"maps": names, "default_map": default_map}))
                with self.assertRaisesRegex(ValueError, "gameplay.json"):
                    list_map_names()

    def test_only_registered_folders_load_and_layout_is_not_required(self):
        settings = map_settings_path("hotel")
        settings.parent.mkdir(parents=True)
        settings.write_text('{"barrier_kinds": []}')
        other = map_settings_path("unregistered")
        other.parent.mkdir()
        other.write_text("{}")
        self.assertEqual(list_map_names(), ["hotel"])
        self.assertEqual(load_map_settings("hotel"), {"barrier_kinds": []})
        self.assertEqual(load_map_barrier_kinds("hotel"), {})
        self.assertFalse(map_layout_path("hotel").exists())
        with self.assertRaisesRegex(ValueError, "not registered"):
            load_map_settings("unregistered")

    def test_missing_malformed_and_incomplete_settings_identify_the_file(self):
        with self.assertRaisesRegex(OSError, "hotel/settings.json"):
            load_map_settings("hotel")
        path = map_settings_path("hotel")
        path.parent.mkdir(parents=True)
        for text in ["{", "[]"]:
            path.write_text(text)
            with self.assertRaisesRegex(ValueError, "hotel/settings.json"):
                load_map_settings("hotel")
        path.write_text("{}")
        with self.assertRaisesRegex(ValueError, "settings.json: barrier_kinds"):
            load_map_barrier_kinds("hotel")

    def test_layout_identity_comes_from_the_folder(self):
        self.assertEqual(map_name_from_path(map_layout_path("hotel")), "hotel")
        with self.assertRaises(ValueError):
            map_name_from_path(map_settings_path("hotel"))
        with self.assertRaises(ValueError):
            map_layout_path("../hotel")


class MapSettingsWindowTests(WindowTestCase):
    def setUp(self):
        super().setUp()
        global_config = json.loads(GAMEPLAY_PATH.read_text())
        settings = {name: map_settings_path(name).read_bytes() for name in ["hotel", "obby"]}
        self.root = Path(self.temp.name)
        self.global_path = self.root / "gameplay.json"
        global_config["maps"] = ["hotel", "obby"]
        global_config["default_map"] = "hotel"
        self.global_path.write_text(json.dumps(global_config))
        for name, data in settings.items():
            directory = self.root / name
            directory.mkdir(exist_ok=True)
            (directory / "settings.json").write_bytes(data)
        for target, value in [
            ("map_editor.constants.GAMEPLAY_PATH", self.global_path),
            ("map_editor.constants.MAPS_DIR", self.root),
            ("map_editor.dependencies.GAMEPLAY_PATH", self.global_path),
            ("map_editor.file_actions.MAPS_DIR", self.root),
        ]:
            override = patch(target, value)
            override.start()
            self.addCleanup(override.stop)
        self.window.refresh_ui()

    def test_save_as_changes_layout_and_catalog_but_preserves_both_settings_files(self):
        before = {name: map_settings_path(name).read_bytes() for name in ["hotel", "obby"]}
        data = self.window.doc.root_data.copy()
        with patch("map_editor.file_actions.QInputDialog.getItem", return_value=("obby", True)):
            self.assertTrue(self.window.save_as())
        self.assertEqual(self.window.path, map_layout_path("obby"))
        self.assertEqual(self.window.catalog_map, "obby")
        self.assertEqual(self.window.barrier_kinds, ["barrier_1"])
        self.assertIn("obby", self.window.windowTitle())
        self.assertEqual(read_map(self.window.path), data)
        self.assertIn(str(map_settings_path("obby").resolve()), self.window.dependencies.watcher.files())
        self.assertNotIn(str(map_settings_path("hotel").resolve()), self.window.dependencies.watcher.files())
        for name, original in before.items():
            self.assertEqual(map_settings_path(name).read_bytes(), original)

    def test_new_registered_map_can_create_a_layout_without_overwriting_settings(self):
        source = map_settings_path("hotel").read_bytes()
        global_config = json.loads(self.global_path.read_text())
        global_config["maps"].append("fresh")
        self.global_path.write_text(json.dumps(global_config))
        settings = map_settings_path("fresh")
        settings.parent.mkdir()
        settings.write_bytes(source)
        with (
            patch("map_editor.file_actions.ResizeMapDialog.prompt", return_value=(8, 8, 0, 0)),
            patch("map_editor.file_actions.QInputDialog.getItem", return_value=("fresh", True)),
        ):
            self.window.new_file()
        self.assertEqual(self.window.path, map_layout_path("fresh"))
        self.assertFalse(self.window.path.exists())
        self.assertTrue(self.window.save())
        self.assertTrue(self.window.path.exists())
        self.assertEqual(settings.read_bytes(), source)

    def test_recent_paths_follow_registered_maps_and_remove_duplicates(self):
        previous = str(self.root / "hotel.json")
        current = str(map_layout_path("hotel"))
        unregistered = str(self.root / "unregistered.json")
        self.window.preferences.setValue(self.window.RECENT_FILES_KEY, [previous, current, unregistered])
        self.assertEqual(self.window._load_recent_paths(), [current, unregistered])
        self.assertEqual(self.window.preferences.value(self.window.RECENT_FILES_KEY), [current, unregistered])

    def test_layout_autosave_recovers_edits_and_preserves_settings(self):
        settings = map_settings_path("hotel").read_bytes()
        self.window.add_floor_rect((2, 2), (2, 2))
        changed = self.window.doc.root_data.copy()
        self.window.doc.write_autosave()
        self.assertEqual(self.window.doc.autosave_path(), self.path.with_name("layout.autosave.json"))
        self.window.undo_stack.undo()
        self.assertTrue(self.window.doc.recover_autosave())
        self.assertEqual(self.window.doc.root_data, changed)
        self.assertEqual(map_settings_path("hotel").read_bytes(), settings)

    def test_atomic_settings_replacement_reloads_and_restores_the_file_watch(self):
        path = map_settings_path("hotel")
        self.assertIn(str(path.resolve()), self.window.dependencies.watcher.files())
        for color in ["#010203", "#040506"]:
            settings = json.loads(path.read_text())
            name = settings["barrier_kinds"][0]["id"]
            settings["barrier_kinds"][0]["color"] = color
            replacement = path.with_suffix(".tmp")
            replacement.write_text(json.dumps(settings))
            replacement.replace(path)
            for _ in range(30):
                if self.window.barrier_kind_colors.get(name) == color:
                    break
                QTest.qWait(100)
            self.assertEqual(self.window.barrier_kind_colors[name], color)
            self.assertIn(str(path.resolve()), self.window.dependencies.watcher.files())
