import copy
from unittest.mock import patch

from editor_fixtures import WindowTestCase, faces, floor
from map_editor.dialogs.resize import ResizeMapDialog
from map_editor.normalization import empty_level, empty_map


class ResizeWindowTests(WindowTestCase):
    def test_shrink_is_staged_and_undoable_without_changing_level_count(self):
        data = empty_map(12, 10)
        data["checkpoints"] = []
        data["levels"] = [empty_level(i) for i in range(3)]
        data["levels"][1]["floors"] = [floor(3, 4)]
        data["levels"][1]["walls"] = [{"c0": 2, "r0": 4, "c1": 3, "r1": 4, **faces()}]
        data["levels"][2]["floors"] = [floor(7, 6)]
        self.window.doc.replace_with_new(data)
        before = copy.deepcopy(self.window.map_data)

        def edit(dialog):
            dialog.shrink.setChecked(True)
            self.assertEqual(dialog.values(), (6, 3, -2, -4))
            self.assertEqual(self.window.map_data, before)
            self.assertEqual(self.window.undo_stack.count(), 0)
            dialog.accept()
            return dialog.result()

        with patch.object(ResizeMapDialog, "exec", edit):
            self.window.resize_map()
        after = copy.deepcopy(self.window.map_data)
        self.assertEqual((after["grid_cols"], after["grid_rows"]), (6, 3))
        self.assertEqual([level["name"] for level in after["levels"]], [level["name"] for level in before["levels"]])
        self.assertEqual(after["levels"][1]["floors"], [floor(1, 0)])
        self.assertEqual(after["levels"][2]["floors"], [floor(5, 2)])
        self.assertEqual(self.window.undo_stack.count(), 1)
        self.window.undo_stack.undo()
        self.assertEqual(self.window.map_data, before)
        self.window.undo_stack.redo()
        self.assertEqual(self.window.map_data, after)

    def test_cancel_and_turning_off_shrink_preserve_manual_settings(self):
        before = copy.deepcopy(self.window.map_data)

        def edit(dialog):
            dialog._cols_spin.setValue(11)
            dialog._rows_spin.setValue(10)
            dialog._anchor_group.button(8).setChecked(True)
            dialog.shrink.setChecked(True)
            self.assertEqual(dialog.values(), (7, 7, -1, -1))
            dialog.shrink.setChecked(False)
            self.assertEqual(dialog.values(), (11, 10, 3, 2))
            dialog.shrink.setChecked(True)
            dialog.reject()
            return dialog.result()

        with patch.object(ResizeMapDialog, "exec", edit):
            self.window.resize_map()
        self.assertEqual(self.window.map_data, before)
        self.assertEqual(self.window.undo_stack.count(), 0)

    def test_manual_resize_still_uses_the_selected_anchor(self):
        before = copy.deepcopy(self.window.map_data)
        for anchor, expected in ((0, (1, 1)), (4, (2, 3)), (8, (3, 5))):
            with self.subTest(anchor=anchor):
                self.window.doc.replace_with_new(before)

                def edit(dialog, anchor=anchor):
                    dialog._cols_spin.setValue(10)
                    dialog._rows_spin.setValue(12)
                    dialog._anchor_group.button(anchor).setChecked(True)
                    dialog.accept()
                    return dialog.result()

                with patch.object(ResizeMapDialog, "exec", edit):
                    self.window.resize_map()
                tile = self.window.map_data["levels"][0]["floors"][0]
                self.assertEqual((tile["col"], tile["row"]), expected)

    def test_shrink_changes_only_the_active_nested_geometry(self):
        data = copy.deepcopy(self.window.map_data)
        child = empty_map(8, 8)
        child["checkpoints"] = []
        child["levels"][0]["floors"] = [floor(2, 3)]
        data["nested_geometry"] = {"room": child, "other": empty_map(2, 2)}
        self.window.doc.replace_with_new(data)
        self.window.doc.select_map("room")
        before = copy.deepcopy(self.window.doc.root_data)

        def edit(dialog):
            dialog.shrink.setChecked(True)
            dialog.accept()
            return dialog.result()

        with patch.object(ResizeMapDialog, "exec", edit):
            self.window.resize_map()
        self.assertEqual((self.window.map_data["grid_cols"], self.window.map_data["grid_rows"]), (1, 1))
        self.assertEqual(self.window.map_data["levels"][0]["floors"], [floor(0, 0)])
        self.assertEqual(self.window.doc.root_data["levels"], before["levels"])
        self.assertEqual(self.window.doc.nested_geometry["other"], before["nested_geometry"]["other"])
        self.window.undo_stack.undo()
        self.assertEqual(self.window.doc.root_data, before)
