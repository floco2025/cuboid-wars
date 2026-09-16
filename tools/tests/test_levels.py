import copy
from unittest.mock import patch

from PySide6.QtWidgets import QDialog, QMessageBox

from editor_fixtures import DEFAULT_ALIAS, WindowTestCase, floor, nested
from map_editor.dialogs.levels import LevelsDialog
from map_editor.io import read_map
from map_editor.normalization import empty_level, empty_map


class LevelsWindowTests(WindowTestCase):
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
            dialog.accept()
            return dialog.result()

        with patch.object(LevelsDialog, "exec", edit):
            window.edit_levels()
        self.assertEqual([level["name"] for level in window.map_data["levels"]], ["Entrance", "Gallery", "Roof"])
        self.assertEqual(window.map_data["items"][0]["level"], 2)
        self.assertEqual(window.current_level, 2)
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
        data["ramps"] = [{"lower_level": 0, "low": [3, 3], "high": [6, 4], "all": DEFAULT_ALIAS}]
        window.doc.replace_with_new(data)
        before = copy.deepcopy(window.map_data)

        def edit(dialog):
            dialog.table.item(0, 1).setText("Renamed")
            dialog.add_button.click()
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

    def test_undoing_a_staged_insertion_restores_ramps_without_a_document_edit(self):
        data = empty_map(8, 8)
        data["levels"].append(empty_level(1))
        data["ramps"] = [{"lower_level": 0, "low": [3, 3], "high": [6, 4], "all": DEFAULT_ALIAS}]
        self.window.doc.replace_with_new(data)
        before = copy.deepcopy(self.window.map_data)

        def edit(dialog):
            dialog.add_button.click()
            self.assertIn("ramps", dialog.summary.text())
            dialog.remove_button.click()
            self.assertEqual(dialog.summary.text(), "")
            dialog.accept()
            return dialog.result()

        with patch.object(LevelsDialog, "exec", edit):
            self.window.edit_levels()
        self.assertEqual(self.window.map_data, before)
        self.assertEqual(self.window.undo_stack.count(), 0)

    def test_last_level_cannot_be_removed(self):
        dialog = LevelsDialog(self.window, self.window.map_data, 0)
        self.assertFalse(dialog.remove_button.isEnabled())
        dialog.remove_level()
        self.assertEqual(dialog.table.rowCount(), 1)
        dialog.add_button.click()
        self.assertTrue(dialog.remove_button.isEnabled())
        dialog.remove_button.click()
        self.assertFalse(dialog.remove_button.isEnabled())
        dialog.reject()
