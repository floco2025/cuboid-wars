import copy
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from editor_fixtures import qt_app
from map_editor.control_catalogs import edit_catalog
from map_editor.dialogs.controls import FieldPropertiesDialog
from map_editor.document import MapDocument
from map_editor.io import read_map, write_map
from map_editor.normalization import empty_map


class ControlTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.app = qt_app()

    def root(self):
        root = empty_map(3, 3)
        root["switch_kinds"] = [{"id": "lobby", "activation": "auto", "reset_on_player_death": "never"}]
        root["_settings"] = {
            "barrier_kinds": [{"id": "green", "color": "#22cc33"}],
            "bridge_kinds": [],
            "movement": {"gravity": 12},
        }
        root["levels"][0]["barriers"] = [{"c0": 0, "r0": 0, "c1": 1, "r1": 0, "kind": "green", "switch": "lobby"}]
        root["items"] = [{"col": 1, "row": 1, "level": 0, "type": "key", "kind": "green"}]
        root["pressure_plates"] = [{"col": col, "row": 2, "level": 0, "switch": "lobby"} for col in (0, 2)]
        nested = copy.deepcopy(root)
        nested.pop("_settings")
        nested.pop("switch_kinds")
        root["nested_geometry"] = {"room": nested}
        root["fireworks"] = {"switch": "lobby", "cooldown_secs": 2}
        return root

    def test_renaming_kinds_updates_placed_and_unplaced_geometry_and_keys(self):
        root = self.root()
        after = edit_catalog(root, "barrier_kinds", [{"id": "blue", "color": "#0000ff"}], {"green": "blue"})
        for geometry in (after, after["nested_geometry"]["room"]):
            self.assertEqual(geometry["items"][0]["kind"], "blue")
            self.assertEqual(geometry["levels"][0]["barriers"][0]["kind"], "blue")
            self.assertNotIn("key_kind", geometry["levels"][0]["barriers"][0])
        after = edit_catalog(
            after,
            "switch_kinds",
            [{"id": "entrance", "activation": "auto", "reset_on_player_death": "never"}],
            {"lobby": "entrance"},
        )
        self.assertEqual(after["fireworks"]["switch"], "entrance")
        self.assertTrue(all(p["switch"] == "entrance" for p in after["pressure_plates"]))
        self.assertEqual(after["nested_geometry"]["room"]["levels"][0]["barriers"][0]["switch"], "entrance")
        self.assertEqual(root["items"][0]["kind"], "green")

    def test_used_kinds_cannot_be_deleted(self):
        for catalog in ("switch_kinds", "barrier_kinds"):
            with self.assertRaisesRegex(ValueError, "still assigned"):
                edit_catalog(self.root(), catalog, [], {})

    def test_bulk_controls_preserve_mixed_appearance_and_choose_one_plate_kind(self):
        dialog = FieldPropertiesDialog(
            None,
            "Fields",
            ["red", "blue"],
            ["a", "b"],
            [{"kind": "red", "switch": "a"}, {"kind": "blue", "switch": "b"}],
        )
        self.assertNotIn("kind", dialog.values())
        self.assertNotIn("switch", dialog.values())
        dialog.control.kind.setCurrentIndex(dialog.control.kind.findData("b"))
        dialog.control.response.setCurrentIndex(dialog.control.response.findData("Off"))
        self.assertEqual(dialog.values(), {"switch": "b", "switch_inverted": True})
        dialog.deleteLater()

    def test_catalog_changes_share_undo_recovery_and_save_without_changing_tuning(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "layout.json"
            settings_path = path.with_name("settings.json")
            root = self.root()
            settings_path.write_text(json.dumps(root.pop("_settings")))
            write_map(path, root)
            doc = MapDocument(path)
            before = copy.deepcopy(doc.root_data)
            after = edit_catalog(before, "barrier_kinds", [{"id": "blue", "color": "#0000ff"}], {"green": "blue"})
            doc.apply_root_change("Rename barrier", after, None)
            doc.undo_stack.undo()
            self.assertEqual(doc.root_data, before)
            doc.undo_stack.redo()
            doc.write_autosave()
            recovered = read_map(doc.autosave_path())
            self.assertEqual(recovered, after)
            doc.write()
            settings = json.loads(settings_path.read_text())
            self.assertEqual(settings["movement"], {"gravity": 12})
            self.assertEqual(settings["barrier_kinds"][0]["id"], "blue")
            self.assertNotIn("_settings", read_map(path))
            self.assertFalse(doc.dirty)

    def test_external_tuning_changes_survive_a_catalog_save(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "layout.json"
            root = self.root()
            settings = root.pop("_settings")
            settings_path = path.with_name("settings.json")
            settings_path.write_text(json.dumps(settings))
            write_map(path, root)
            doc = MapDocument(path)
            after = edit_catalog(doc.root_data, "barrier_kinds", [{"id": "green", "color": "#ff0000"}], {})
            doc.apply_root_change("Recolor", after, None)
            settings["movement"]["gravity"] = 7
            settings_path.write_text(json.dumps(settings))
            self.assertTrue(doc.externally_modified())
            doc.write()
            self.assertEqual(json.loads(settings_path.read_text())["movement"]["gravity"], 7)

    def test_failed_layout_save_restores_settings_and_keeps_edits_unsaved(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "layout.json"
            root = self.root()
            settings = root.pop("_settings")
            settings_path = path.with_name("settings.json")
            settings_path.write_text(json.dumps(settings))
            write_map(path, root)
            original_layout = path.read_text()
            doc = MapDocument(path)
            after = edit_catalog(doc.root_data, "barrier_kinds", [{"id": "blue", "color": "#0000ff"}], {"green": "blue"})
            doc.apply_root_change("Rename barrier", after, None)
            with patch("map_editor.document.write_map", side_effect=OSError("write failed")):
                with self.assertRaisesRegex(OSError, "write failed"):
                    doc.write()
            self.assertEqual(json.loads(settings_path.read_text()), settings)
            self.assertEqual(path.read_text(), original_layout)
            self.assertEqual(doc.root_data, after)
            self.assertTrue(doc.dirty)
