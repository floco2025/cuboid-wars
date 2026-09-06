import copy
import tempfile
import unittest
from pathlib import Path
from unittest.mock import Mock, patch

from PySide6.QtCore import QEvent, QMimeData, QPoint, QSettings, Qt
from PySide6.QtGui import QKeySequence
from PySide6.QtTest import QTest
from PySide6.QtWidgets import QApplication, QMessageBox

from map_editor.constants import DEFAULT_ALIAS, MODE_ERASE, MODE_FLOOR, MODE_SELECT
from map_editor.document import MapDocument
from map_editor.io import empty_map, write_map
from map_editor.normalization import canonicalize_map
from map_editor.nested_maps import NestedMapShape, nested_map_cycle
from map_editor.regions import GLOBAL_LISTS, LEVEL_LISTS, TileRegion, copy_region, delete_region, paste_region
from map_editor.select import CLIPBOARD_MIME
from map_editor.window import EditorWindow
from map_editor.validation import validate_map


def furnished_block() -> dict:
    data = empty_map(4, 4)
    data["levels"].append(empty_map()["levels"][0])
    data["levels"][0].update({
        "floors": [{"col": 0, "row": 0, "all": DEFAULT_ALIAS}],
        "inaccessible_floors": [{"col": 1, "row": 0, "all": DEFAULT_ALIAS}],
        "grass": [{"col": 0, "row": 0}],
        "walls": [{"c0": 0, "r0": 0, "c1": 1, "r1": 0, "all": DEFAULT_ALIAS}],
        "barriers": [{"c0": 3, "r0": 0, "c1": 4, "r1": 0, "kind": "gate"}],
        "light_bridges": [{"col": 2, "row": 0, "kind": "bridge"}],
        "lights": [{"col": 0, "row": 0, "side": "N"}],
    })
    data["actor_spawn_zones"] = [{"level": 0, "cols": [0, 1], "rows": [0, 1], "kind": "beetle", "count": 2}]
    data["player_spawn_zones"] = [{"level": 1, "cols": [0, 1], "rows": [0, 1]}]
    data["items"] = [{"level": 0, "col": 0, "row": 0, "type": "cookie"}]
    data["pressure_plates"] = [{"level": 0, "col": 0, "row": 0, "type": "firework"}]
    data["ramps"] = [{"lower_level": 0, "low": [0, 2], "high": [3, 3], "all": DEFAULT_ALIAS}]
    data["ladders"] = [{"lower_level": 0, "col": 3, "row": 3, "side": "N", "levels": 1}]
    data["nested_maps"] = [{"map": "tile", "level": 0, "from": [1, 1], "to": [2, 1], "to_level": 1}]
    return canonicalize_map(data)


class RegionTests(unittest.TestCase):
    def test_every_object_family_survives_copy_paste_at_another_position_and_level(self):
        block = furnished_block()
        snapshot = copy.deepcopy(block)
        destination = empty_map(12, 12)
        destination["player_spawn_zones"] = []
        pasted = canonicalize_map(paste_region(destination, block, (5, 6), 1))
        recovered = copy_region(pasted, TileRegion((5, 6, 9, 10), 1, 2))
        for name in GLOBAL_LISTS:
            self.assertEqual(recovered[name], block[name], name)
        for index, level in enumerate(block["levels"]):
            for name in LEVEL_LISTS:
                self.assertEqual(recovered["levels"][index][name], level[name], name)
        self.assertEqual(len(pasted["levels"]), 3)
        self.assertEqual(block, snapshot)
        self.assertEqual(len(destination["levels"]), 1)

    def test_delete_clears_all_families_and_leaves_the_other_levels_alone(self):
        data = furnished_block()
        data["levels"].append(copy.deepcopy(data["levels"][0]))
        remaining = delete_region(data, TileRegion((0, 0, 4, 4), 0, 2))
        for name in GLOBAL_LISTS:
            self.assertEqual(remaining[name], [], name)
        for level in remaining["levels"][:2]:
            for name in LEVEL_LISTS:
                self.assertEqual(level[name], [], name)
        self.assertEqual(remaining["levels"][2], data["levels"][2])

    def test_paste_replaces_empty_cells_too_and_preserves_neighbors(self):
        data = empty_map(8, 8)
        data["player_spawn_zones"] = []
        data["levels"][0]["floors"] = [{"col": c, "row": 3, "all": DEFAULT_ALIAS} for c in (2, 3, 4)]
        data["items"] = [{"level": 0, "col": 3, "row": 3, "type": "cookie"}]
        block = empty_map(2, 1)
        block["player_spawn_zones"] = []
        result = canonicalize_map(paste_region(data, block, (2, 3), 0))
        self.assertEqual([(f["col"], f["row"]) for f in result["levels"][0]["floors"]], [(4, 3)])
        self.assertEqual(result["items"], [])

    def test_boundary_edges_are_copied_and_replaced(self):
        data = empty_map(4, 4)
        data["player_spawn_zones"] = []
        edges = [(1, 1, 2, 1), (1, 2, 2, 2), (1, 1, 1, 2), (2, 1, 2, 2)]
        data["levels"][0]["walls"] = [dict(zip(("c0", "r0", "c1", "r1"), e)) for e in edges]
        region = TileRegion((1, 1, 2, 2), 0)
        self.assertEqual(len(copy_region(data, region)["levels"][0]["walls"]), 4)
        self.assertEqual(delete_region(data, region)["levels"][0]["walls"], [])

    def test_partial_multicell_and_multilevel_objects_are_rejected_without_mutation(self):
        data = furnished_block()
        before = copy.deepcopy(data)
        for rect, levels, message in [((0, 2, 2, 3), 2, "ramp"), ((0, 2, 4, 4), 1, "ramp"),
                                      ((3, 3, 4, 4), 1, "ladder"), ((1, 1, 2, 2), 2, "nested map")]:
            with self.subTest(message=message, rect=rect, levels=levels):
                with self.assertRaisesRegex(ValueError, message):
                    delete_region(data, TileRegion(rect, 0, levels))
        self.assertEqual(data, before)

    def test_paste_outside_grid_is_rejected_without_clipping_or_changes(self):
        data = empty_map(4, 4)
        before = copy.deepcopy(data)
        with self.assertRaisesRegex(ValueError, "does not fit"):
            paste_region(data, furnished_block(), (1, 0), 0)
        self.assertEqual(data, before)

    def test_deleting_a_shared_wall_cannot_silently_lose_an_unselected_light(self):
        data = empty_map(4, 4)
        data["player_spawn_zones"] = []
        data["levels"][0]["walls"] = [{"c0": 1, "r0": 1, "c1": 2, "r1": 1}]
        data["levels"][0]["lights"] = [{"col": 1, "row": 0, "side": "S"}]
        region = TileRegion((1, 1, 2, 2), 0)
        block = copy_region(data, region)
        with self.assertRaisesRegex(ValueError, "boundary wall"):
            delete_region(data, region)
        pasted = paste_region(data, block, (1, 1), 0)
        self.assertEqual(pasted["levels"][0]["lights"], data["levels"][0]["lights"])

    def test_partial_spawn_zone_does_not_get_split_or_duplicate_actor_counts(self):
        data = empty_map(8, 8)
        with self.assertRaisesRegex(ValueError, "spawn zone"):
            copy_region(data, TileRegion((0, 0, 1, 1), 0))


class WindowTests(unittest.TestCase):
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
        self.app.processEvents()

    def tearDown(self):
        with patch.object(self.window, "confirm_discard_changes", return_value=True):
            self.window.close()
        self.window.deleteLater()
        self.app.sendPostedEvents(None, QEvent.Type.DeferredDelete)
        self.recents.stop()
        self.temp.cleanup()

    def click(self, col, row):
        size = self.window.canvas.cell_size()
        QTest.mouseClick(self.window.canvas, Qt.MouseButton.LeftButton, pos=QPoint(round((col + .5) * size), round((row + .5) * size)))

    def test_click_and_reverse_drag_select_tiles_and_enable_menus(self):
        window = self.window
        self.assertEqual(window.mode, MODE_SELECT)
        self.assertFalse(window.copy_action.isEnabled())
        self.click(1, 1)
        self.assertEqual(window.tile_selection, (1, 1, 2, 2))
        self.assertTrue(window.copy_action.isEnabled())
        self.assertTrue(window.cut_action.isEnabled())
        self.assertTrue(window.delete_action.isEnabled())
        self.assertFalse(window.paste_action.isEnabled())
        size = window.canvas.cell_size()
        QTest.mousePress(window.canvas, Qt.MouseButton.LeftButton, pos=QPoint(round(4.5 * size), round(3.5 * size)))
        QTest.mouseRelease(window.canvas, Qt.MouseButton.LeftButton, pos=QPoint(round(2.5 * size), round(1.5 * size)))
        self.assertEqual(window.tile_selection, (2, 1, 5, 4))
        self.assertFalse(window.dirty)

    def test_copy_cut_paste_delete_and_undo_keep_clipboard_and_saved_state_correct(self):
        window = self.window
        before = copy.deepcopy(window.map_data)
        self.click(1, 1)
        with patch("map_editor.select.QInputDialog.getInt", return_value=(1, True)) as prompt:
            window.copy_action.trigger()
            self.assertEqual(prompt.call_args.args[3:6], (1, 1, 1))
        self.assertEqual(window.map_data, before)
        self.assertFalse(window.dirty)
        self.assertTrue(window.paste_action.isEnabled())
        with patch("map_editor.select.QInputDialog.getInt", return_value=(1, True)):
            window.cut_action.trigger()
        self.assertEqual(window.map_data["levels"][0]["floors"], [])
        self.assertEqual(window.undo_stack.count(), 1)
        clipboard = bytes(self.app.clipboard().mimeData().data(CLIPBOARD_MIME))
        window.undo_stack.undo()
        self.assertEqual(window.map_data, before)
        self.assertFalse(window.dirty)
        self.click(4, 4)
        window.paste_action.trigger()
        self.assertEqual(len(window.map_data["levels"][0]["floors"]), 2)
        with patch("map_editor.select.QInputDialog.getInt", return_value=(1, True)):
            window.delete_action.trigger()
        self.assertEqual(window.map_data, before)
        self.assertEqual(bytes(self.app.clipboard().mimeData().data(CLIPBOARD_MIME)), clipboard)
        window.undo_stack.undo()
        self.assertEqual(len(window.map_data["levels"][0]["floors"]), 2)
        window.undo_stack.redo()
        self.assertEqual(window.map_data, before)

    def test_cancelled_dialog_and_switching_tool_do_not_edit_the_map(self):
        self.click(1, 1)
        before = copy.deepcopy(self.window.map_data)
        with patch("map_editor.select.QInputDialog.getInt", return_value=(1, False)):
            self.window.cut_selection()
        self.assertEqual(self.window.map_data, before)
        self.assertIsNone(self.window.tile_clipboard)
        self.window.mode_combo.setCurrentText(MODE_FLOOR)
        self.assertFalse(self.window.copy_action.isEnabled())
        self.assertIsNone(self.window.canvas.hover_target)

    def test_failed_save_as_preserves_file_identity_and_dirty_state(self):
        self.window.add_floor_rect((2, 2), (2, 2))
        destination = Path(self.temp.name) / "another.json"
        with patch("map_editor.document.write_map", side_effect=OSError("disk full")), patch("map_editor.file_actions.QMessageBox.critical"):
            self.assertFalse(self.window._save_to(destination, {}, {}, .1))
        self.assertEqual(self.window.path, self.path)
        self.assertTrue(self.window.dirty)
        self.assertFalse(destination.exists())

    def test_save_rejects_nested_map_errors_before_writing(self):
        self.window.map_data["nested_maps"] = canonicalize_map({**empty_map(), "nested_maps": [{
            "map": "map", "level": 0, "from": [0, 0], "to": [0, 0],
        }]})["nested_maps"]
        with patch("map_editor.file_actions.QMessageBox.warning") as warning, patch.object(self.window.doc, "write") as write:
            self.assertFalse(self.window.save())
        write.assert_not_called()
        self.assertIn("nests the edited map itself", warning.call_args.args[2])

    def test_standard_shortcuts_copy_paste_delete_and_deselect(self):
        self.window.activateWindow()
        self.click(1, 1)
        self.app.processEvents()
        with patch("map_editor.select.QInputDialog.getInt", return_value=(1, True)) as prompt:
            QTest.keySequence(self.window.canvas, QKeySequence(QKeySequence.StandardKey.Copy))
            prompt.assert_called_once()
        self.click(5, 5)
        QTest.keySequence(self.window.canvas, QKeySequence(QKeySequence.StandardKey.Paste))
        self.assertEqual(len(self.window.map_data["levels"][0]["floors"]), 2)
        with patch("map_editor.select.QInputDialog.getInt", return_value=(1, True)) as prompt:
            QTest.keyClick(self.window.canvas, Qt.Key.Key_Backspace)
            prompt.assert_called_once()
        QTest.keySequence(self.window.canvas, QKeySequence(QKeySequence.StandardKey.SelectAll))
        self.assertEqual(self.window.tile_selection, (0, 0, 8, 8))
        QTest.keyClick(self.window.canvas, Qt.Key.Key_Escape)
        self.assertIsNone(self.window.tile_selection)
        self.assertFalse(self.window.delete_action.isEnabled())

    def test_escape_cancels_an_erase_drag_before_release(self):
        window = self.window
        window.activateWindow()
        window.mode_combo.setCurrentText(MODE_ERASE)
        size = window.canvas.cell_size()
        pos = QPoint(round(1.5 * size), round(1.5 * size))
        QTest.mousePress(window.canvas, Qt.MouseButton.LeftButton, pos=pos)
        self.app.processEvents()
        QTest.keyClick(window.canvas, Qt.Key.Key_Escape)
        QTest.mouseRelease(window.canvas, Qt.MouseButton.LeftButton, pos=pos)
        self.assertEqual(len(window.map_data["levels"][0]["floors"]), 1)
        self.assertFalse(window.dirty)

    def test_multilevel_paste_extends_map_and_undo_removes_added_levels(self):
        window = self.window
        window.add_level()
        window.set_level_index(0)
        self.click(1, 1)
        with patch("map_editor.select.QInputDialog.getInt", return_value=(2, True)):
            window.copy_selection()
        window.set_level_index(1)
        self.click(4, 4)
        window.paste_selection()
        self.assertEqual(len(window.map_data["levels"]), 3)
        self.assertEqual(window.map_data["levels"][1]["floors"][0]["col"], 4)
        window.undo_stack.undo()
        self.assertEqual(len(window.map_data["levels"]), 2)
        self.assertEqual(window.map_data["levels"][1]["floors"], [])

    def test_clipboard_survives_opening_a_map_but_selection_does_not(self):
        self.click(1, 1)
        with patch("map_editor.select.QInputDialog.getInt", return_value=(1, True)):
            self.window.copy_selection()
        block = copy.deepcopy(self.window.tile_clipboard)
        other = Path(self.temp.name) / "other.json"
        write_map(other, empty_map(8, 8))
        self.window.load_path(other)
        self.assertIsNone(self.window.tile_selection)
        self.assertEqual(self.window.tile_clipboard, block)
        self.assertFalse(self.window.paste_action.isEnabled())
        self.click(4, 4)
        self.assertTrue(self.window.paste_action.isEnabled())
        self.window.paste_selection()
        self.assertEqual(self.window.map_data["levels"][0]["floors"][0]["col"], 4)

    def test_invalid_clipboard_disables_paste(self):
        self.click(1, 1)
        mime = QMimeData()
        mime.setData(CLIPBOARD_MIME, b'{"map": null}')
        self.app.clipboard().setMimeData(mime)
        self.assertFalse(self.window.paste_action.isEnabled())

    def test_tool_and_level_navigation_do_not_revalidate_unchanged_data(self):
        self.window.add_level()
        with patch("map_editor.window.validate_map") as validate:
            self.window.set_level_index(0)
            self.window.mode_combo.setCurrentText(MODE_FLOOR)
        validate.assert_not_called()

    def test_failed_file_open_keeps_recovery_copy_of_current_work(self):
        window = self.window
        window.add_floor_rect((2, 2), (2, 2))
        window.doc.write_autosave()
        with patch("map_editor.file_actions.QMessageBox.critical"):
            window.load_path(Path(self.temp.name) / "missing.json")
        self.assertEqual(window.path, self.path)
        self.assertTrue(window.doc.autosave_path().exists())

    def test_resize_reports_pressure_plates_even_when_no_geometry_is_lost(self):
        self.window.map_data["pressure_plates"] = [{"level": 0, "col": 7, "row": 7, "type": "firework"}]
        before = copy.deepcopy(self.window.map_data)
        with patch("map_editor.structure.ResizeMapDialog.prompt", return_value=(6, 6, 0, 0)), patch(
            "map_editor.structure.QMessageBox.question", return_value=QMessageBox.StandardButton.Cancel,
        ) as question:
            self.window.resize_map()
        self.assertIn("1 pressure plates", question.call_args.args[2])
        self.assertEqual(self.window.map_data, before)


class DocumentTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.app = QApplication.instance() or QApplication([])

    def test_unsaved_document_starts_dirty(self):
        self.assertTrue(MapDocument(None).dirty)

    def test_save_as_failure_keeps_autosave_and_original_identity(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "original.json"
            write_map(path, empty_map())
            doc = MapDocument(path)
            doc.dirty = True
            doc.write_autosave()
            with patch("map_editor.document.write_map", side_effect=OSError("disk full")):
                with self.assertRaises(OSError):
                    doc.write(Path(temp) / "another.json")
            self.assertEqual(doc.path, path)
            self.assertTrue(doc.dirty)
            self.assertTrue(doc.autosave_path().exists())

    def test_one_tile_map_has_an_in_bounds_spawn_zone(self):
        self.assertEqual(validate_map(empty_map(1, 1), [], []), [])

    def test_nested_cycle_check_visits_a_shared_dependency_once(self):
        graph = {
            "left": NestedMapShape(1, 1, 1, ("shared",)),
            "right": NestedMapShape(1, 1, 1, ("shared",)),
            "shared": NestedMapShape(1, 1, 1, ()),
        }
        lookup = Mock(side_effect=graph.get)
        self.assertIsNone(nested_map_cycle("root", [{"map": "left"}, {"map": "right"}], lookup))
        self.assertEqual([call.args[0] for call in lookup.call_args_list].count("shared"), 1)
