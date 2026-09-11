import copy
import json
import io
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from editor_fixtures import DEFAULT_ALIAS, WindowTestCase, qt_app
from map_editor.catalogs import load_texture_catalog
from map_editor.app import main
from config_fixtures import ConfigTestCase
from map_editor.document import MapDocument
from map_editor.editing import paint_floors
from map_editor.io import read_map, write_map
from map_editor.normalization import empty_map, normalize_map, normalize_nested_map
from map_editor.window import EditorWindow


def placement(name, col=0):
    return normalize_nested_map({"map": name, "level": 0, "from": [col, 0], "to": [col, 0]})


def parent_map():
    root = empty_map(8, 8)
    room = empty_map(3, 2)
    room["player_spawn_zones"] = []
    room["levels"][0]["floors"] = [{"col": 1, "row": 1, "all": DEFAULT_ALIAS}]
    root["nested_geometry"] = {"room": room, "platform": empty_map(1, 1)}
    root["nested_maps"] = [placement("room"), placement("room", 4)]
    return normalize_map(root)


class NestedDocumentTests(ConfigTestCase):
    @classmethod
    def setUpClass(cls):
        qt_app()

    def test_edits_across_maps_share_undo_save_and_recovery(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "hotel.json"
            original = parent_map()
            write_map(path, original)
            doc = MapDocument(path)
            doc.select_map("room")
            self.assertFalse(doc.dirty)
            self.assertEqual(doc.undo_stack.count(), 0)
            doc.apply_change("Paint room", paint_floors(doc.map_data, 0, (0, 0, 1, 1), DEFAULT_ALIAS))
            doc.select_map(None)
            doc.apply_change("Paint outer", paint_floors(doc.map_data, 0, (2, 2, 3, 3), DEFAULT_ALIAS))
            changed = copy.deepcopy(doc.root_data)
            doc.select_map("platform")
            doc.write_autosave()
            self.assertEqual(read_map(doc.autosave_path()), changed)
            recovered = MapDocument(path)
            self.assertTrue(recovered.recover_autosave())
            self.assertEqual(recovered.root_data, changed)
            self.assertTrue(recovered.dirty)
            doc.undo_stack.undo()
            self.assertIsNone(doc.active_map)
            doc.undo_stack.undo()
            self.assertEqual(doc.active_map, "room")
            self.assertEqual(doc.root_data, original)
            self.assertFalse(doc.dirty)
            doc.undo_stack.redo()
            doc.undo_stack.redo()
            doc.select_map("room")
            doc.write()
            self.assertEqual(doc.active_map, "room")
            self.assertEqual(read_map(path), changed)
            self.assertFalse(doc.dirty)
            self.assertFalse(doc.autosave_path().exists())

    def test_repairs_cover_every_nested_definition(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "hotel.json"
            data = parent_map()
            data["nested_geometry"]["room"]["ladders"] = [{"lower_level": 0, "col": 1, "row": 0, "side": "N", "levels": 3}]
            write_map(path, data)
            doc = MapDocument(path)
            repaired, summary = doc.proposed_repairs()
            self.assertEqual(summary, ["Nested room: ladders: remove/change 1, add/change 0"])
            self.assertTrue(doc.apply_repairs(repaired))
            self.assertEqual(doc.nested_geometry["room"]["ladders"], [])
            self.assertEqual(doc.root_data["nested_maps"], data["nested_maps"])
            doc.undo_stack.undo()
            self.assertEqual(doc.root_data, data)

    def test_cli_requires_settings_before_opening_a_window(self):
        with patch.object(sys, "argv", ["editor.py", "unregistered"]), patch("sys.stderr", new_callable=io.StringIO) as stderr:
            with self.assertRaises(SystemExit) as failure:
                main()
        self.assertEqual(failure.exception.code, 2)
        self.assertIn("not registered", stderr.getvalue())



class NestedWindowTests(WindowTestCase):
    def setUp(self):
        super().setUp()
        write_map(self.path, parent_map())
        self.window.doc.load(self.path)

    def select(self, name):
        combo = self.window.map_combo
        combo.setCurrentIndex(combo.findData(name))

    def test_selector_uses_parent_catalogs_and_clears_selection(self):
        window = self.window
        catalogs = (window.barrier_kind_colors.copy(), window.bridge_kind_colors.copy(),
                    window.texture_catalog.copy(), window.wall_width_cells)
        window.tile_selection = (4, 4, 5, 5)
        self.select("room")
        self.assertEqual(window.map_data["grid_cols"], 3)
        self.assertIsNone(window.tile_selection)
        self.assertFalse(window.dirty)
        self.assertEqual((window.barrier_kind_colors, window.bridge_kind_colors,
                          window.texture_catalog, window.wall_width_cells), catalogs)
        window.apply_change("Paint", paint_floors(window.map_data, 0, (0, 0, 1, 1), DEFAULT_ALIAS))
        self.assertTrue(window.save())
        self.assertEqual(window.doc.active_map, "room")
        self.assertEqual(read_map(self.path)["nested_geometry"]["room"], window.map_data)
        self.select(None)
        self.assertEqual(window.map_data["grid_cols"], 8)

    def test_catalog_reload_keeps_the_parent_while_editing_nested_geometry(self):
        self.select("room")
        with patch("map_editor.catalogs.load_texture_catalog", wraps=load_texture_catalog) as textures:
            self.window.reload_dependencies()
        textures.assert_called_with("hotel")
        self.assertEqual(self.window.doc.active_map, "room")

    def test_failed_catalog_reload_keeps_the_document_editable(self):
        self.select("room")
        before = copy.deepcopy(self.window.doc.root_data)
        with patch("map_editor.catalogs.load_texture_catalog", side_effect=ValueError("invalid catalog")):
            self.window.reload_dependencies()
        self.assertEqual(self.window.doc.root_data, before)
        self.assertEqual(self.window.doc.active_map, "room")

    def test_nested_names_are_local_to_the_parent_document(self):
        data = parent_map()
        data["nested_geometry"]["hotel"] = data["nested_geometry"].pop("room")
        data["nested_maps"] = [placement("hotel")]
        self.assertFalse(self.window.validate_document(data))

    def test_inactive_nested_issues_block_save_and_navigate_to_the_geometry(self):
        window = self.window
        self.select("room")
        after = copy.deepcopy(window.map_data)
        after["levels"][0]["floors"][0]["top"] = "missing-alias"
        window.apply_change("Invalid material", after)
        self.select(None)
        with patch("map_editor.file_actions.QMessageBox.warning") as warning:
            self.assertFalse(window.save())
        self.assertIn("Nested room", warning.call_args.args[2])
        issue = next(issue for issue in window.validate_document(window.doc.root_data).issues
                     if "missing-alias" in issue.message)
        window.focus_issue(issue)
        self.assertEqual(window.doc.active_map, "room")
        self.assertEqual(window.canvas.issue_rects, [(1, 1, 2, 2)])
        self.assertNotEqual(read_map(self.path), window.doc.root_data)

    def test_renaming_updates_every_placement_and_undo_restores_names(self):
        window = self.window
        self.select("platform")
        after = copy.deepcopy(window.map_data)
        after["nested_maps"] = [placement("room")]
        window.apply_change("Nest room", after)
        self.select("room")
        with patch("map_editor.nested_definitions.QInputDialog.getText", return_value=("cabin", True)):
            window.rename_nested_map()
        self.assertEqual(window.doc.active_map, "cabin")
        self.assertNotIn("room", window.doc.nested_geometry)
        self.assertEqual([entry["map"] for entry in window.doc.root_data["nested_maps"]], ["cabin", "cabin"])
        self.assertEqual(window.doc.nested_geometry["platform"]["nested_maps"][0]["map"], "cabin")
        window.undo_stack.undo()
        self.assertEqual(window.doc.active_map, "room")
        self.assertEqual(window.doc.root_data["nested_maps"][0]["map"], "room")

    def test_create_delete_and_undo_keep_the_parent_document(self):
        window = self.window
        with (
            patch("map_editor.nested_definitions.QInputDialog.getText", return_value=("lift", True)),
            patch("map_editor.nested_definitions.ResizeMapDialog.prompt", return_value=(2, 1, 0, 0)),
        ):
            window.new_nested_map()
        self.assertEqual(window.doc.active_map, "lift")
        self.assertEqual(window.map_data["grid_cols"], 2)
        self.assertEqual(window.map_data["player_spawn_zones"], [])
        self.assertEqual(window.path, self.path)
        window.delete_nested_map()
        self.assertIsNone(window.doc.active_map)
        self.assertNotIn("lift", window.doc.nested_geometry)
        window.undo_stack.undo()
        self.assertEqual(window.doc.active_map, "lift")
        self.select("room")
        with patch("map_editor.nested_definitions.QMessageBox.warning") as warning:
            window.delete_nested_map()
        self.assertIn("Outer map", warning.call_args.args[2])
        self.assertIn("room", window.doc.nested_geometry)

    def test_shapes_follow_unsaved_nested_resizing_and_undo(self):
        window = self.window
        self.select("room")
        after = copy.deepcopy(window.map_data)
        after["grid_cols"] = 6
        window.apply_change("Widen room", after)
        self.select(None)
        self.assertEqual(window.nested_map_shape("room").grid_cols, 6)
        window.undo_stack.undo()
        self.assertEqual(window.nested_map_shape("room").grid_cols, 3)

    def test_open_and_save_as_reject_missing_settings_without_changing_document(self):
        window = self.window
        before = copy.deepcopy(window.doc.root_data)
        other = self.path.parent.parent / "unregistered" / "layout.json"
        write_map(other, empty_map())
        with self.assertRaisesRegex(ValueError, "not registered"):
            EditorWindow(other)
        with patch("map_editor.file_actions.QMessageBox.critical"):
            window.load_path(other)
            self.assertFalse(window._save_to(other))
        self.assertEqual(window.doc.root_data, before)
        self.assertEqual(window.path, self.path)

    def test_missing_references_and_cycles_are_checked_across_the_document(self):
        data = copy.deepcopy(self.window.doc.root_data)
        data["nested_geometry"]["room"]["nested_maps"] = [placement("platform")]
        data["nested_geometry"]["platform"]["nested_maps"] = [placement("room")]
        self.assertTrue(any("loop" in error for error in self.window.validate_document(data)))
        data["nested_geometry"]["platform"]["nested_maps"] = [placement("missing")]
        self.assertTrue(any("named geometry is missing" in error for error in self.window.validate_document(data)))
