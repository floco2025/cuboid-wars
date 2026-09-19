import copy
import json
from unittest.mock import patch

from editor_fixtures import DEFAULT_ALIAS, WindowTestCase, floor, nested
from map_editor.catalogs import map_settings_path
from map_editor.dialogs.levels import LevelsDialog
from map_editor.io import read_map
from map_editor.normalization import empty_level, empty_map
from PySide6.QtWidgets import QDialog, QMessageBox


class LevelsWindowTests(WindowTestCase):
    def test_shrink_scales_vertical_nudges_by_floor_thickness(self):
        path = map_settings_path("hotel")
        settings = json.loads(path.read_text())
        settings["geometry"].update(level_height=4, floor_thickness=1, wall_thickness=0.75)
        path.write_text(json.dumps(settings))
        self.window.reload_dependencies()
        data = empty_map(8, 8)
        data["checkpoints"] = []
        data["levels"] = [empty_level(i) for i in range(7)]
        data["nested_geometry"] = {"platform": empty_map(2, 2)}
        data["nested_maps"] = [nested("platform", 1, [2, 2], [2, 2])]
        data["nested_maps"][0]["from_nudge"][1] = 16
        data["nested_maps"][0]["to_nudge"][1] = 16
        self.window.doc.replace_with_new(data)

        def edit(dialog):
            dialog.shrink_button.click()
            self.assertEqual(dialog.values(), [(i, f"Level {i}") for i in range(1, 6)])
            dialog.accept()
            return dialog.result()

        with patch.object(LevelsDialog, "exec", edit):
            self.window.edit_levels()
        self.assertEqual(len(self.window.map_data["levels"]), 5)
        self.assertEqual(self.window.map_data["nested_maps"][0]["from_nudge"], [0, 16, 0])

    def test_shrink_keeps_interior_levels_names_spans_and_nested_motion(self):
        data = empty_map(8, 8)
        data["checkpoints"] = []
        data["levels"] = [empty_level(i) for i in range(8)]
        data["levels"][2]["floors"] = [floor(2, 2)]
        data["items"] = [{"level": 5, "col": 3, "row": 3, "type": "gold"}]
        child = empty_map(2, 2)
        child["levels"].append(empty_level(1))
        data["nested_geometry"] = {"cabin": child}
        data["nested_maps"] = [nested("cabin", 3, [4, 4], [5, 5], 5)]
        self.window.doc.replace_with_new(data)
        self.window.set_level_index(5)
        before = copy.deepcopy(self.window.map_data)

        def edit(dialog):
            dialog.table.item(2, 1).setText("Landing")
            dialog.table.setCurrentCell(7, 1)
            dialog.add_button.click()
            dialog.table.setCurrentCell(5, 1)
            dialog.shrink_button.click()
            self.assertEqual(dialog.values(), [(i, "Landing" if i == 2 else f"Level {i}") for i in range(2, 7)])
            self.assertEqual(dialog.summary.text(), "")
            self.assertEqual(self.window.map_data, before)
            dialog.accept()
            return dialog.result()

        with patch.object(LevelsDialog, "exec", edit):
            self.window.edit_levels()
        after = copy.deepcopy(self.window.map_data)
        self.assertEqual((after["grid_cols"], after["grid_rows"]), (8, 8))
        self.assertEqual(len(after["levels"]), 5)
        self.assertEqual(after["levels"][0]["floors"], before["levels"][2]["floors"])
        self.assertEqual(after["items"][0], {"level": 3, "col": 3, "row": 3, "type": "gold"})
        self.assertEqual((after["nested_maps"][0]["level"], after["nested_maps"][0]["to_level"]), (1, 3))
        self.assertEqual(after["nested_geometry"], before["nested_geometry"])
        self.assertEqual(self.window.current_level, 3)
        self.assertEqual(self.window.undo_stack.count(), 1)
        self.window.undo_stack.undo()
        self.assertEqual(self.window.map_data, before)
        self.window.undo_stack.redo()
        self.assertEqual(self.window.map_data, after)

    def test_shrink_empty_levels_keeps_one_and_cancel_discards_it(self):
        data = empty_map(8, 8)
        data["checkpoints"] = []
        data["levels"] = [empty_level(i) for i in range(4)]
        self.window.doc.replace_with_new(data)
        self.window.set_level_index(3)
        before = copy.deepcopy(self.window.map_data)

        def edit(dialog):
            dialog.shrink_button.click()
            self.assertEqual(dialog.values(), [(0, "Level 0")])
            self.assertFalse(dialog.remove_button.isEnabled())
            dialog.shrink_button.click()
            self.assertEqual(dialog.table.rowCount(), 1)
            dialog.reject()
            return dialog.result()

        with patch.object(LevelsDialog, "exec", edit):
            self.window.edit_levels()
        self.assertEqual(self.window.map_data, before)
        self.assertEqual(self.window.current_level, 3)
        self.assertEqual(self.window.undo_stack.count(), 0)

    def test_batch_names_add_remove_save_and_undo_in_the_active_nested_map(self):
        window = self.window
        child = empty_map(8, 8)
        child["levels"] += [empty_level(1), empty_level(2)]
        child["items"] = [{"level": 2, "col": 2, "row": 2, "type": "gold"}]
        root = copy.deepcopy(window.map_data)
        root["nested_geometry"] = {"room": child, "other": empty_map(2, 2)}
        window.doc.replace_with_new(root)
        window.doc.select_map("room")
        before = copy.deepcopy(window.doc.root_data)

        def edit(dialog):
            dialog.table.item(0, 1).setText("Entrance")
            dialog.table.setCurrentCell(1, 1)
            dialog.remove_button.click()
            dialog.table.setCurrentCell(0, 1)
            dialog.add_button.click()
            dialog.table.cellWidget(1, 1).setText("Gallery")
            dialog.table.setCurrentCell(2, 1)
            dialog.table.editItem(dialog.table.item(2, 1))
            dialog.table.cellWidget(2, 1).setText("  Roof  ")
            self.assertFalse(dialog.down_button.isEnabled())
            dialog.up_button.click()
            self.assertEqual(dialog.table.currentRow(), 1)
            self.assertEqual(dialog.values(), [(0, "Entrance"), (2, "  Roof  "), (None, "Gallery")])
            self.assertTrue(dialog.up_button.isEnabled())
            self.assertTrue(dialog.down_button.isEnabled())
            dialog.up_button.click()
            self.assertFalse(dialog.up_button.isEnabled())
            dialog.down_button.click()
            self.assertEqual(window.doc.root_data, before)
            dialog.accept()
            return dialog.result()

        with patch.object(LevelsDialog, "exec", edit):
            window.edit_levels()
        self.assertEqual([level["name"] for level in window.map_data["levels"]], ["Entrance", "Roof", "Gallery"])
        self.assertEqual(window.map_data["items"][0]["level"], 1)
        self.assertEqual(window.current_level, 1)
        self.assertEqual(window.undo_stack.count(), 1)
        self.assertEqual(window.undo_stack.undoText(), "Edit Levels")
        after = copy.deepcopy(window.doc.root_data)
        self.assertEqual(after["levels"], before["levels"])
        self.assertEqual(after["nested_geometry"]["other"], before["nested_geometry"]["other"])
        window.undo_stack.undo()
        self.assertEqual(window.doc.root_data, before)
        window.undo_stack.redo()
        self.assertEqual(window.doc.root_data, after)
        window.doc.write(self.path)
        self.assertEqual(read_map(self.path), after)

    def test_cancel_keeps_document_history_and_current_level_untouched(self):
        window = self.window
        data = empty_map(8, 8)
        data["levels"].append(empty_level(1))
        data["ramps"] = [{"lower_level": 0, "cols": [3, 6], "rows": [3, 4], "direction": "E", "all": DEFAULT_ALIAS}]
        window.doc.replace_with_new(data)
        before = copy.deepcopy(window.map_data)

        def edit(dialog):
            dialog.table.item(0, 1).setText("Renamed")
            dialog.down_button.click()
            self.assertIn("1 ramps", dialog.summary.text())
            with patch(
                "map_editor.dialogs.levels.QMessageBox.question", return_value=QMessageBox.StandardButton.Cancel
            ):
                dialog.accept()
            self.assertEqual(dialog.result(), QDialog.DialogCode.Rejected)
            self.assertEqual(window.map_data, before)
            dialog.reject()
            return dialog.result()

        with patch.object(LevelsDialog, "exec", edit):
            window.edit_levels()
        self.assertEqual(window.map_data, before)
        self.assertEqual(window.undo_stack.count(), 0)
        self.assertEqual(window.current_level, 0)

    def test_removal_lists_dropped_geometry_and_one_undo_restores_it(self):
        window = self.window
        data = empty_map(8, 8)
        data["levels"] += [empty_level(1), empty_level(2)]
        data["levels"][1]["floors"] = [floor(2, 2)]
        data["checkpoints"] = [{"level": 1, "cols": [2, 3], "rows": [2, 3], "type": "individual"}]
        data["nested_geometry"] = {"platform": empty_map(1, 1)}
        data["nested_maps"] = [nested("platform", 0, [4, 4], [6, 4], 2)]
        data["items"] = [{"level": 2, "col": 1, "row": 1, "type": "gold"}]
        window.doc.replace_with_new(data)
        window.set_level_index(1)
        before = copy.deepcopy(window.map_data)

        def edit(dialog):
            self.assertEqual(dialog.table.currentRow(), 1)
            dialog.remove_button.click()
            for dropped in ("1 floors", "1 checkpoints", "1 nested maps"):
                self.assertIn(dropped, dialog.summary.text())
            with patch(
                "map_editor.dialogs.levels.QMessageBox.question", return_value=QMessageBox.StandardButton.Yes
            ) as confirm:
                dialog.accept()
            confirm.assert_called_once()
            return dialog.result()

        with patch.object(LevelsDialog, "exec", edit):
            window.edit_levels()
        self.assertEqual(len(window.map_data["levels"]), 2)
        self.assertEqual(window.map_data["checkpoints"], [])
        self.assertEqual(window.map_data["nested_maps"], [])
        self.assertEqual(window.map_data["items"][0]["level"], 1)
        self.assertEqual(window.current_level, 1)
        window.undo_stack.undo()
        self.assertEqual(window.map_data, before)

    def test_a_staged_insertion_grows_a_ramp_and_undoing_it_needs_no_document_edit(self):
        data = empty_map(8, 8)
        data["levels"].append(empty_level(1))
        data["ramps"] = [{"lower_level": 0, "cols": [3, 6], "rows": [3, 4], "direction": "E", "all": DEFAULT_ALIAS}]
        self.window.doc.replace_with_new(data)
        before = copy.deepcopy(self.window.map_data)

        def edit(dialog):
            dialog.add_button.click()
            self.assertEqual(dialog.summary.text(), "")
            self.assertEqual(dialog.edited_data()["ramps"][0]["levels"], 2)
            dialog.remove_button.click()
            self.assertEqual(dialog.edited_data(), before)
            dialog.accept()
            return dialog.result()

        with patch.object(LevelsDialog, "exec", edit):
            self.window.edit_levels()
        self.assertEqual(self.window.map_data, before)
        self.assertEqual(self.window.undo_stack.count(), 0)

    def test_reversing_moves_restores_ramps_and_cancel_discards_reordering(self):
        data = empty_map(8, 8)
        data["levels"].append(empty_level(1))
        data["ramps"] = [{"lower_level": 0, "cols": [3, 6], "rows": [3, 4], "direction": "E", "all": DEFAULT_ALIAS}]
        self.window.doc.replace_with_new(data)
        before = copy.deepcopy(self.window.map_data)

        def edit(dialog):
            dialog.down_button.click()
            self.assertIn("1 ramps", dialog.summary.text())
            dialog.up_button.click()
            self.assertEqual(dialog.summary.text(), "")
            self.assertEqual(dialog.edited_data(), before)
            dialog.down_button.click()
            with patch(
                "map_editor.dialogs.levels.QMessageBox.question", return_value=QMessageBox.StandardButton.Cancel
            ) as confirm:
                dialog.accept()
            confirm.assert_called_once()
            self.assertEqual(dialog.result(), QDialog.DialogCode.Rejected)
            dialog.reject()
            return dialog.result()

        with patch.object(LevelsDialog, "exec", edit):
            self.window.edit_levels()
        self.assertEqual(self.window.map_data, before)
        self.assertEqual(self.window.undo_stack.count(), 0)
        self.assertEqual(self.window.current_level, 0)

    def test_last_level_cannot_be_removed(self):
        dialog = LevelsDialog(self.window, self.window.map_data, 0)
        self.assertFalse(dialog.remove_button.isEnabled())
        self.assertFalse(dialog.up_button.isEnabled())
        self.assertFalse(dialog.down_button.isEnabled())
        dialog.remove_level()
        self.assertEqual(dialog.table.rowCount(), 1)
        dialog.add_button.click()
        self.assertTrue(dialog.remove_button.isEnabled())
        dialog.remove_button.click()
        self.assertFalse(dialog.remove_button.isEnabled())
        dialog.reject()
