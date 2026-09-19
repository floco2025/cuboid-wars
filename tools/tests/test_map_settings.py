import copy
import json
import os
from unittest.mock import patch

from config_fixtures import ConfigTestCase
from editor_fixtures import WindowTestCase
from map_editor.catalogs import (
    list_map_names,
    load_map_geometry,
    load_map_settings,
    map_layout_path,
    map_name_from_path,
    map_settings_path,
    switch_colors,
)
from map_editor.dialogs.levels import LevelsDialog
from map_editor.io import read_map, write_map
from map_editor.normalization import empty_map
from PySide6.QtTest import QTest
from PySide6.QtWidgets import QMessageBox


class MapSettingsTests(ConfigTestCase):
    def setUp(self):
        super().setUp()
        self.global_path.write_text(json.dumps({"default_map": "hotel", "maps": ["hotel"]}))
        override = patch("map_editor.catalogs.MAPS_DIR", self.root / "maps")
        override.start()
        self.addCleanup(override.stop)

    def test_registry_rejects_invalid_names_duplicates_and_default(self):
        for names, default_map in [
            ([], "hotel"),
            ([""], ""),
            (["../hotel"], "../hotel"),
            (["hotel", "hotel"], "hotel"),
            ({"hotel": {}}, "hotel"),
            ([1], "hotel"),
            (["hotel"], "missing"),
            (["hotel"], []),
        ]:
            with self.subTest(names=names, default_map=default_map):
                self.global_path.write_text(json.dumps({"maps": names, "default_map": default_map}))
                with self.assertRaisesRegex(ValueError, "gameplay.json"):
                    list_map_names()

    def test_only_registered_folders_load_and_layout_is_not_required(self):
        settings = map_settings_path("hotel")
        settings.parent.mkdir(parents=True)
        settings.write_text('{"grounds": null, "random_items": null, "placed_items": null}')
        other = map_settings_path("unregistered")
        other.parent.mkdir()
        other.write_text("{}")
        self.assertEqual(list_map_names(), ["hotel"])
        self.assertEqual(load_map_settings("hotel"), {"grounds": None, "random_items": None, "placed_items": None})
        self.assertFalse(map_layout_path("hotel").exists())
        with self.assertRaisesRegex(ValueError, "not registered"):
            load_map_settings("unregistered")

    def test_settings_merge_over_the_defaults_and_reject_unknown_keys(self):
        self.global_path.write_text(
            json.dumps(
                {
                    "default_map": "hotel",
                    "maps": ["hotel"],
                    "movement": {"gravity": 25, "player": {"run_speed": 9}},
                    "power_ups": {"speed": {"mode": "pickup", "duration_secs": 30}},
                }
            )
        )
        settings = map_settings_path("hotel")
        settings.parent.mkdir(parents=True)
        content = {"grounds": None, "random_items": None, "placed_items": None}
        settings.write_text(
            json.dumps({**content, "movement": {"gravity": 24}, "power_ups": {"speed": {"mode": "always"}}})
        )
        self.assertEqual(
            load_map_settings("hotel"),
            {
                "movement": {"gravity": 24, "player": {"run_speed": 9}},
                "power_ups": {"speed": {"mode": "always"}},
                **content,
            },
        )
        settings.write_text(json.dumps({**content, "movement": {"playr": {}}}))
        with self.assertRaisesRegex(ValueError, "hotel/settings.json: movement.playr is not a key in the defaults"):
            load_map_settings("hotel")

    def test_missing_malformed_and_incomplete_settings_identify_the_file(self):
        with self.assertRaisesRegex(OSError, "hotel/settings.json"):
            load_map_settings("hotel")
        path = map_settings_path("hotel")
        path.parent.mkdir(parents=True)
        for text in ["{", "[]"]:
            path.write_text(text)
            with self.assertRaisesRegex(ValueError, "hotel/settings.json"):
                load_map_settings("hotel")

    def test_switch_colors_follow_the_first_field_on_the_switch_and_explicit_switch_colors(self):
        data = empty_map(3, 3)
        data["fields"] = [
            {"id": "plain", "color": "#ffffff"},
            {"id": "cyan", "color": "#30d8ff", "switch": "bridge"},
            {"id": "green", "color": "#22cc33", "switch": "door"},
            {"id": "red", "color": "#ff3333", "switch": "bridge", "initially_on": False},
            {"id": "gold", "color": "#f0c020", "switch": "show"},
        ]
        data["switches"] = [
            {"id": "door"},
            {"id": "bridge"},
            {"id": "show", "color": "#9b5de5"},
            {"id": "other"},
        ]
        colors = switch_colors(data)
        self.assertEqual(colors, {"door": "#22cc33", "bridge": "#30d8ff", "show": "#9b5de5", "other": "#2c99bc"})
        data["switches"][0]["color"] = "#ffaa00"
        self.assertEqual(switch_colors(data)["door"], "#ffaa00")

    def test_layout_identity_comes_from_the_folder(self):
        self.assertEqual(map_name_from_path(map_layout_path("hotel")), "hotel")
        with self.assertRaises(ValueError):
            map_name_from_path(map_settings_path("hotel"))
        with self.assertRaises(ValueError):
            map_layout_path("../hotel")


class MapSettingsWindowTests(WindowTestCase):
    def test_invalid_geometry_reload_retains_last_valid_dimensions_for_level_editing(self):
        window = self.window
        path = map_settings_path("hotel")
        settings = json.loads(path.read_text())
        expected = window.grid_cell_size, window.level_height, window.wall_width_cells, window.floor_thickness
        for key in ("grid_cell_size", "level_height", "wall_thickness", "floor_thickness"):
            for value in (0, -1, float("nan"), float("inf"), "4", None, True):
                with self.subTest(key=key, value=value):
                    invalid = copy.deepcopy(settings)
                    invalid["geometry"][key] = value
                    path.write_text(json.dumps(invalid))
                    with self.assertRaisesRegex(ValueError, f"geometry.{key}"):
                        load_map_geometry("hotel")
        settings["geometry"]["level_height"] = 0
        path.write_text(json.dumps(settings))
        window.reload_dependencies()
        self.assertIn("geometry.level_height", window.canvas.notice.text())
        self.assertEqual(
            (window.grid_cell_size, window.level_height, window.wall_width_cells, window.floor_thickness), expected
        )

        def edit(dialog):
            dialog.shrink_button.click()
            dialog.reject()
            return dialog.result()

        with patch.object(LevelsDialog, "exec", edit):
            window.edit_levels()

    def test_save_as_carries_the_layouts_catalogs_and_leaves_both_settings_files_alone(self):
        before = {name: map_settings_path(name).read_bytes() for name in ["hotel", "obby"]}
        edited = copy.deepcopy(self.window.doc.root_data)
        edited["fields"] = [{"id": "edited", "color": "#123456"}, {"id": "light", "color": "#654321"}]
        self.window.doc.apply_root_change("Edit catalogs", edited, None)
        data = copy.deepcopy(self.window.doc.root_data)
        with patch("map_editor.file_actions.QInputDialog.getItem", return_value=("obby", True)):
            self.assertTrue(self.window.save_as())
        self.assertEqual(self.window.path, map_layout_path("obby"))
        self.assertEqual(self.window.catalog_map, "obby")
        self.assertEqual(self.window.field_colors, {"edited": "#123456", "light": "#654321"})
        self.assertIn("obby", self.window.windowTitle())
        self.assertEqual(read_map(self.window.path), data)
        self.assertIn(str(map_settings_path("obby").resolve()), self.window.dependencies.watcher.files())
        self.assertNotIn(str(map_settings_path("hotel").resolve()), self.window.dependencies.watcher.files())
        for name, content in before.items():
            self.assertEqual(map_settings_path(name).read_bytes(), content)

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
        self.assertEqual(json.loads(settings.read_text()), json.loads(source))

    def test_new_with_a_malformed_settings_file_reports_and_keeps_the_document(self):
        global_config = json.loads(self.global_path.read_text())
        global_config["maps"].append("fresh")
        self.global_path.write_text(json.dumps(global_config))
        settings = map_settings_path("fresh")
        settings.parent.mkdir()
        settings.write_text('{"portals": "both",}')
        original = self.window.doc.root_data.copy()
        with (
            patch("map_editor.file_actions.ResizeMapDialog.prompt", return_value=(8, 8, 0, 0)),
            patch("map_editor.file_actions.QInputDialog.getItem", return_value=("fresh", True)),
            patch("map_editor.file_actions.QMessageBox.critical") as critical,
        ):
            self.window.new_file()
        critical.assert_called_once()
        self.assertIn("fresh/settings.json", critical.call_args.args[2])
        self.assertEqual(self.window.path, self.path)
        self.assertEqual(self.window.doc.root_data, original)
        self.assertEqual(self.window.catalog_map, "hotel")

    def test_an_invalid_settings_reload_keeps_the_catalogs_and_reports(self):
        window = self.window
        path = map_settings_path("hotel")
        window.reload_dependencies()
        materials = list(window.materials_catalog)
        settings = json.loads(path.read_text())
        settings["textures"] = []
        path.write_text(json.dumps(settings))
        window.reload_dependencies()
        self.assertIn("Catalog reload failed", window.canvas.notice.text())
        self.assertEqual(list(window.materials_catalog), materials)
        window.add_floor_rect((2, 2), (2, 2))
        window.refresh_ui()
        self.assertEqual(list(window.materials_catalog), materials)

    def test_new_over_an_existing_layout_replaces_it_only_after_asking(self):
        obby = map_layout_path("obby")
        existing = {"fireworks": None, **empty_map(5, 5)}
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
        external = {"fireworks": None, **empty_map(12, 12)}
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
        for alias in ["fresh-one", "fresh-two"]:
            settings = json.loads(path.read_text())
            settings["textures"][alias] = {"material": "test", "portalable": True}
            replacement = path.with_suffix(".tmp")
            replacement.write_text(json.dumps(settings))
            replacement.replace(path)
            for _ in range(30):
                if alias in self.window.texture_catalog:
                    break
                QTest.qWait(100)
            self.assertIn(alias, self.window.texture_catalog)
            self.assertIn(str(path.resolve()), self.window.dependencies.watcher.files())
