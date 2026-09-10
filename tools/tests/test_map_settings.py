import json
import os
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from PySide6.QtTest import QTest
from PySide6.QtWidgets import QMessageBox

from editor_fixtures import WindowTestCase
from map_editor.catalogs import (
    list_map_names,
    load_map_barrier_kinds,
    load_map_settings,
    load_map_switches,
    load_map_plate_colors,
    map_layout_path,
    map_name_from_path,
    map_settings_path,
)
from map_editor.constants import GAMEPLAY_PATH
from map_editor.io import read_map, write_map
from map_editor.normalization import empty_map


class MapSettingsTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.global_path = self.root / "gameplay.json"
        self.global_path.write_text(json.dumps({"default_map": "hotel", "maps": ["hotel"]}))
        for target, value in [
            ("map_editor.catalogs.GAMEPLAY_PATH", self.global_path),
            ("map_editor.catalogs.MAPS_DIR", self.root / "maps"),
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
        with self.assertRaisesRegex(ValueError, "settings.json: switches is required"):
            load_map_switches("hotel")

    def test_switch_catalog_keeps_its_order_and_rejects_bad_policies(self):
        path = map_settings_path("hotel")
        path.parent.mkdir(parents=True)
        good = [
            {"id": "lobby", "activation": "auto", "reset_on_player_death": "never"},
            {"id": "finale", "activation": "momentary", "reset_on_player_death": "all", "held": "everyone"},
        ]
        path.write_text(json.dumps({"switches": good}))
        self.assertEqual(load_map_switches("hotel"), ["lobby", "finale"])
        for bad, message in [
            ([{"id": "", "activation": "auto", "reset_on_player_death": "never"}], "id is empty"),
            (good + [good[0]], "duplicates 'lobby'"),
            ([{"id": "a", "activation": "hold", "reset_on_player_death": "never"}], "activation must be one of"),
            ([{"id": "a", "activation": "auto", "reset_on_player_death": "always"}], "reset_on_player_death must be"),
            ([{"id": "a", "activation": "auto", "reset_on_player_death": "never", "held": "all"}], "held must be"),
            ([{"id": "a", "activation": "auto", "reset_on_player_death": "never", "plate_color": "red"}], "plate_color must look like"),
            ([{"activation": "auto", "reset_on_player_death": "never"}], "string `id`"),
            ({}, "must be an array"),
        ]:
            path.write_text(json.dumps({"switches": bad}))
            with self.assertRaisesRegex(ValueError, message):
                load_map_switches("hotel")

    def test_plate_colors_follow_targets_and_explicit_switch_colors(self):
        path = map_settings_path("hotel")
        path.parent.mkdir(parents=True)
        path.write_text(json.dumps({
            "switches": [{"id": "door"}, {"id": "bridge"}, {"id": "show", "plate_color": "#9b5de5"}, {"id": "other"}],
            "barrier_kinds": [{"id": "green", "color": "#22cc33", "switch": "door"}],
            "bridge_kinds": [{"id": "cyan", "color": "#30d8ff", "switch": "bridge"}],
        }))
        colors = load_map_plate_colors("hotel")
        self.assertEqual(colors, {"door": "#22cc33", "bridge": "#30d8ff", "show": "#9b5de5", "other": "#2c99bc"})
        settings = json.loads(path.read_text())
        settings["switches"][0]["plate_color"] = "#ffaa00"
        path.write_text(json.dumps(settings))
        self.assertEqual(load_map_plate_colors("hotel")["door"], "#ffaa00")

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
            ("map_editor.catalogs.GAMEPLAY_PATH", self.global_path),
            ("map_editor.catalogs.MAPS_DIR", self.root),
            ("map_editor.dependencies.GAMEPLAY_PATH", self.global_path),
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

    def test_new_over_an_existing_layout_replaces_it_only_after_asking(self):
        obby = map_layout_path("obby")
        existing = empty_map(5, 5)
        write_map(obby, existing)
        autosave = obby.with_name("layout.autosave.json")
        autosave.write_text("{}")
        original = self.window.doc.root_data.copy()
        for answer in [QMessageBox.StandardButton.Cancel, QMessageBox.StandardButton.Yes]:
            with (
                patch("map_editor.file_actions.ResizeMapDialog.prompt", return_value=(8, 8, 0, 0)),
                patch("map_editor.file_actions.QInputDialog.getItem", return_value=("obby", True)),
                patch("map_editor.file_actions.QMessageBox.question", return_value=answer) as question,
            ):
                self.window.new_file()
            question.assert_called_once()
            self.assertEqual(read_map(obby), existing)
            self.assertTrue(autosave.exists())
            if answer == QMessageBox.StandardButton.Cancel:
                self.assertEqual(self.window.path, self.path)
                self.assertEqual(self.window.doc.root_data, original)
        self.assertEqual(self.window.path, obby)
        self.assertEqual(self.window.doc.path_mtime, obby.stat().st_mtime)
        self.assertTrue(self.window.dirty)
        self.assertTrue(self.window.save())
        self.assertEqual(read_map(obby)["grid_cols"], 8)
        self.assertFalse(autosave.exists())

    def test_new_warns_before_overwriting_later_external_edits(self):
        path = map_layout_path("obby")
        write_map(path, empty_map(5, 5))
        with (
            patch("map_editor.file_actions.ResizeMapDialog.prompt", return_value=(8, 8, 0, 0)),
            patch("map_editor.file_actions.QInputDialog.getItem", return_value=("obby", True)),
            patch("map_editor.file_actions.QMessageBox.question", return_value=QMessageBox.StandardButton.Yes),
        ):
            self.window.new_file()

        mtime = path.stat().st_mtime
        external = empty_map(12, 12)
        write_map(path, external)
        os.utime(path, (mtime + 1, mtime + 1))
        for answer in [QMessageBox.StandardButton.Cancel, QMessageBox.StandardButton.Yes]:
            with patch("map_editor.file_actions.QMessageBox.question", return_value=answer) as question:
                self.assertEqual(self.window.save(), answer == QMessageBox.StandardButton.Yes)
            question.assert_called_once()
            self.assertEqual(question.call_args.args[1], "File Changed Externally")
            if answer == QMessageBox.StandardButton.Cancel:
                self.assertEqual(read_map(path), external)
                self.assertTrue(self.window.dirty)
        self.assertEqual(read_map(path)["grid_cols"], 8)
        self.assertFalse(self.window.dirty)

    def test_recent_paths_remove_duplicates(self):
        current = str(map_layout_path("hotel"))
        unregistered = str(self.root / "unregistered.json")
        self.window.preferences.setValue(self.window.RECENT_FILES_KEY, [current, unregistered, current])
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
