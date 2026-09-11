import copy
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from PySide6.QtTest import QSignalSpy

from editor_fixtures import DEFAULT_ALIAS, qt_app
from map_editor.document import MapDocument
from map_editor.editing import paint_floors
from map_editor.erasing import erase_cell_rect
from map_editor.io import read_map, write_map
from map_editor.normalization import empty_map
from map_editor.transforms import insert_level_data, resize_map_data
from map_editor.validation import validate_map


class DocumentTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        qt_app()

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.directory = Path(self.temp.name)
        self.path = self.directory / "map.json"
        write_map(self.path, empty_map(8, 8))
        self.doc = MapDocument(self.path, recovery_dir=self.directory / "recovery")

    def tearDown(self):
        self.doc.clear_autosave()
        self.temp.cleanup()

    def test_unsaved_document_starts_dirty(self):
        self.assertTrue(MapDocument(None).dirty)

    def test_save_as_failure_keeps_autosave_and_original_identity(self):
        path = self.directory / "original.json"
        write_map(path, empty_map())
        doc = MapDocument(path)
        doc.dirty = True
        doc.write_autosave()
        with patch("map_editor.document.write_map", side_effect=OSError("disk full")):
            with self.assertRaises(OSError):
                doc.write(self.directory / "another.json")
        self.assertEqual(doc.path, path)
        self.assertTrue(doc.dirty)
        self.assertTrue(doc.autosave_path().exists())

    def test_document_owns_transactions_and_signals_without_a_window(self):
        before = copy.deepcopy(self.doc.map_data)
        changed = QSignalSpy(self.doc.changed)
        after = paint_floors(before, 0, (2, 2, 3, 3), DEFAULT_ALIAS)
        self.assertEqual(self.doc.map_data, before)
        self.assertTrue(self.doc.apply_change("Paint", after))
        self.assertTrue(self.doc.dirty)
        self.assertEqual(changed.count(), 1)
        self.assertEqual(changed.at(0)[0], before)
        self.doc.undo_stack.undo()
        self.assertEqual(self.doc.map_data, before)
        self.assertFalse(self.doc.dirty)
        self.doc.undo_stack.redo()
        self.assertTrue(self.doc.dirty)
        self.assertFalse(self.doc.apply_change("No change", self.doc.map_data))
        self.assertEqual(self.doc.undo_stack.count(), 1)

    def load_damaged_map(self):
        data = empty_map(8, 8)
        data["levels"][0]["lights"] = [{"col": 2, "row": 2, "side": "invalid"}]
        data["ladders"] = [{"col": 3, "row": 3, "lower_level": 0, "levels": 0, "side": "invalid"}]
        data["items"] = [{"col": 7, "row": 7, "level": 0, "type": "gold"}]
        data["actor_spawn_zones"] = [
            {"level": 0, "cols": [0, 1], "rows": [0, 1], "kind": "unknown", "count": -2, "respawn_secs": 90}
        ]
        write_map(self.path, data)
        self.doc.load(self.path)
        return self.doc.map_data

    def test_loading_preserves_invalid_records_and_reports_them(self):
        data = self.load_damaged_map()
        self.assertEqual(len(data["items"]), 1)
        self.assertEqual(data["ladders"][0]["levels"], 0)
        self.assertEqual(data["levels"][0]["lights"][0]["side"], "INVALID")
        errors = validate_map(data, [], [], actor_kinds=["beetle"])
        self.assertTrue(any("unknown actor kind" in error for error in errors))
        self.assertTrue(any("negative count" in error for error in errors))
        self.assertFalse(self.doc.dirty)

    def test_explicit_repairs_are_one_undoable_edit(self):
        before = copy.deepcopy(self.load_damaged_map())
        repaired, summary = self.doc.proposed_repairs()
        self.assertTrue(any("lights" in line for line in summary))
        self.assertTrue(any("items" in line for line in summary))
        self.doc.apply_repairs(repaired)
        self.assertEqual(self.doc.map_data["levels"][0]["lights"], [])
        self.assertTrue(self.doc.dirty)
        self.doc.undo_stack.undo()
        self.assertEqual(self.doc.map_data, before)
        self.assertFalse(self.doc.dirty)

    def test_unrelated_edits_and_coordinate_transforms_do_not_repair_records(self):
        self.load_damaged_map()
        self.doc.apply_change("Paint", paint_floors(self.doc.map_data, 0, (4, 4, 5, 5), DEFAULT_ALIAS))
        self.assertEqual(len(self.doc.map_data["items"]), 1)
        moved = resize_map_data(self.doc.map_data, 10, 10, 2, 2)
        self.doc.apply_change("Resize", moved)
        self.assertEqual(self.doc.map_data["levels"][0]["lights"][0]["col"], 4)
        self.doc.apply_change("Insert", insert_level_data(self.doc.map_data, 0))
        self.assertEqual(self.doc.map_data["levels"][1]["lights"][0]["side"], "INVALID")
        self.assertEqual(self.doc.map_data["items"][0]["level"], 1)

    def test_erasing_a_wall_takes_its_light_even_while_repairs_are_pending(self):
        self.load_damaged_map()
        data = copy.deepcopy(self.doc.map_data)
        data["levels"][0]["walls"] = [{"c0": 2, "r0": 2, "c1": 3, "r1": 2, "all": DEFAULT_ALIAS}]
        data["levels"][0]["lights"].append({"col": 2, "row": 2, "side": "N"})
        self.doc.replace_with_new(data)
        self.doc.apply_change("Erase", erase_cell_rect(self.doc.map_data, 0, (2, 2), (2, 2), True))
        sides = [light["side"] for light in self.doc.map_data["levels"][0]["lights"]]
        self.assertEqual(sides, ["INVALID"])

    def test_an_edit_that_only_reorders_a_file_ordered_map_is_not_an_edit(self):
        data = empty_map(6, 6)
        data["player_spawn_zones"] = []
        data["levels"][0]["floors"] = [
            {"col": 3, "row": 3, "all": DEFAULT_ALIAS},
            {"col": 1, "row": 1, "all": DEFAULT_ALIAS},
        ]
        write_map(self.path, data)
        doc = MapDocument(self.path, recovery_dir=self.directory / "recovery")
        self.assertFalse(doc.apply_change("Paint", paint_floors(doc.map_data, 0, (1, 1, 2, 2), DEFAULT_ALIAS)))
        self.assertFalse(doc.dirty)
        self.assertEqual(doc.undo_stack.count(), 0)

    def test_untitled_recovery_preserves_data_and_rejects_an_active_session(self):
        self.doc.replace_with_new(empty_map())
        self.doc.write_autosave()
        recovery = self.doc.autosave_path()
        self.assertTrue(recovery.exists())
        other = MapDocument(None, recovery_dir=self.directory / "other")
        self.assertFalse(other.recover_session(recovery))
        self.doc.recovery_lock.unlock()
        self.doc.recovery_lock = None
        self.assertTrue(other.recover_session(recovery))
        self.assertEqual(other.map_data, self.doc.map_data)
        self.assertIsNone(other.path)
        self.assertTrue(other.dirty)
        destination = self.directory / "recovered.json"
        other.write(destination)
        self.assertFalse(recovery.exists())
        self.assertEqual(read_map(destination), other.map_data)

    def test_failed_recovery_leaves_current_document_untouched(self):
        before = copy.deepcopy(self.doc.map_data)
        with self.assertRaises(FileNotFoundError):
            self.doc.recover_session(self.directory / "missing.json")
        self.assertEqual(self.doc.map_data, before)
        self.assertEqual(self.doc.path, self.path)

    def test_inserting_through_ramps_requires_removal_and_undo_restores_everything(self):
        data = empty_map()
        data["levels"].append(copy.deepcopy(data["levels"][0]))
        data["ramps"] = [{"lower_level": 0, "low": [2, 2], "high": [5, 3], "all": DEFAULT_ALIAS}]
        data["ladders"] = [{"lower_level": 0, "levels": 1, "col": 6, "row": 6, "side": "N"}]
        self.doc.replace_with_new(data)
        before = copy.deepcopy(self.doc.map_data)
        with self.assertRaises(ValueError):
            insert_level_data(before, 1)
        after = insert_level_data(before, 1, remove_crossing_ramps=True)
        self.assertEqual(after["ramps"], [])
        self.assertEqual(after["ladders"][0]["levels"], 2)
        self.doc.apply_change("Insert", after)
        self.assertEqual(len(self.doc.map_data["levels"]), 3)
        self.doc.undo_stack.undo()
        self.assertEqual(self.doc.map_data, before)
