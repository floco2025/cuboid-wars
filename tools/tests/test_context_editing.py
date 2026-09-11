import copy
from unittest.mock import patch

from PySide6.QtCore import QPointF
from PySide6.QtGui import QContextMenuEvent
from PySide6.QtWidgets import QDialog, QMenu

from editor_fixtures import DEFAULT_ALIAS, WindowTestCase
from map_editor.constants import MODE_ERASE_KEEP_FLOORS, MODE_LIGHT_BRIDGE, MODE_SELECT
from map_editor.dialogs.controls import FieldPropertiesDialog
from map_editor.normalization import empty_level, empty_map


# The field dialog runs for real; `kind` picks an appearance, None cancels.
def field_dialog(kind):
    def run(dialog):
        if kind is None:
            return QDialog.DialogCode.Rejected
        dialog.appearance.setCurrentIndex(dialog.appearance.findData(kind))
        return QDialog.DialogCode.Accepted

    return patch.object(FieldPropertiesDialog, "exec", run)


class ContextEditingTests(WindowTestCase):
    def choose_action(self, point, title):
        canvas = self.window.canvas
        position = canvas.viewport.from_grid(QPointF(*point)).toPoint()
        menu = QMenu(canvas)

        def choose(*_):
            action = next((a for a in menu.actions() if a.text() == title), None)
            self.assertIsNotNone(action, [a.text() for a in menu.actions()])
            action.trigger()

        menu.exec = choose
        with patch("map_editor.canvas.QMenu", return_value=menu):
            canvas.contextMenuEvent(
                QContextMenuEvent(QContextMenuEvent.Reason.Mouse, position, canvas.mapToGlobal(position))
            )
        menu.deleteLater()

    def test_kind_edits_target_one_record_in_the_viewed_map_and_level(self):
        window = self.window
        window.bridge_kind_colors = window.barrier_kind_colors = {"a": "#ff0000", "b": "#0000ff"}
        window.wall_light_kinds = ["a", "b"]
        cases = (
            ("light_bridges", {"col": 2, "row": 3, "kind": "a"}, (2.5, 3.5), "Edit Light Bridge..."),
            ("barriers", {"c0": 2, "r0": 3, "c1": 3, "r1": 3, "kind": "a"}, (2.5, 3.0), "Edit Barrier..."),
            ("lights", {"col": 2, "row": 3, "side": "N", "kind": "a"}, (2.5, 3.05), "Edit Light..."),
        )
        for name, record, point, title in cases:
            with self.subTest(name=name):
                data = empty_map(8, 8)
                level = data["levels"][0]
                neighbor = {**record, "col": 3} if "col" in record else {**record, "c0": 3, "c1": 4}
                level[name] = [record, neighbor]
                if name == "lights":
                    level["floors"] = [{"col": col, "row": 3, "all": DEFAULT_ALIAS} for col in (2, 3)]
                    level["walls"] = [
                        {"c0": col, "r0": 3, "c1": col + 1, "r1": 3, "all": DEFAULT_ALIAS} for col in (2, 3)
                    ]
                data["levels"].append({**copy.deepcopy(level), "name": "Upper"})
                root = copy.deepcopy(data)
                root["nested_geometry"] = {"room": data}
                window.doc.replace_with_new(root)
                window.doc.select_map("room")
                window.set_level_index(1)
                before = copy.deepcopy(window.doc.root_data)
                expected = copy.deepcopy(before)
                expected["nested_geometry"]["room"]["levels"][1][name][0]["kind"] = "b"
                for mode in (MODE_SELECT, MODE_LIGHT_BRIDGE, MODE_ERASE_KEEP_FLOORS):
                    with self.subTest(mode=mode):
                        window.set_mode(mode)
                        with (
                            patch("map_editor.dialogs.KindDialog.prompt", return_value="b") as prompt,
                            field_dialog("b"),
                        ):
                            self.choose_action(point, title)
                        if name == "lights":
                            self.assertEqual(prompt.call_args.args[3], "a")
                        self.assertEqual(window.doc.root_data, expected)
                        window.undo_stack.undo()
                        self.assertEqual(window.doc.root_data, before)
                        window.undo_stack.redo()
                        self.assertEqual(window.doc.root_data, expected)
                        window.undo_stack.undo()

    def test_cancelled_and_unchanged_kind_edits_leave_history_untouched(self):
        window = self.window
        window.bridge_kind_colors = window.barrier_kind_colors = {"a": "#ff0000"}
        window.wall_light_kinds = ["a"]
        data = empty_map(8, 8)
        level = data["levels"][0]
        level["light_bridges"] = [{"col": 2, "row": 3, "kind": "a"}]
        level["barriers"] = [{"c0": 2, "r0": 4, "c1": 3, "r1": 4, "kind": "a"}]
        level["floors"] = [{"col": 4, "row": 3, "all": DEFAULT_ALIAS}]
        level["walls"] = [{"c0": 4, "r0": 3, "c1": 5, "r1": 3, "all": DEFAULT_ALIAS}]
        level["lights"] = [{"col": 4, "row": 3, "side": "N", "kind": "a"}]
        window.doc.replace_with_new(data)
        before = copy.deepcopy(window.map_data)
        for choice in (None, "a"):
            for point, title in (
                ((2.5, 3.5), "Edit Light Bridge..."),
                ((2.5, 4.0), "Edit Barrier..."),
                ((4.5, 3.05), "Edit Light..."),
            ):
                with (
                    self.subTest(choice=choice, title=title),
                    patch("map_editor.dialogs.KindDialog.prompt", return_value=choice),
                    field_dialog(choice),
                ):
                    self.choose_action(point, title)
                    self.assertEqual(window.map_data, before)
                    self.assertEqual(window.undo_stack.count(), 0)

    def test_material_edits_target_the_picked_surface_including_a_ramps_upper_level(self):
        window = self.window
        data = empty_map(8, 8)
        data["levels"].append(empty_level(1))
        level = data["levels"][1]
        level["floors"] = [{"col": 2, "row": 3, "all": DEFAULT_ALIAS}]
        level["inaccessible_floors"] = [{"col": 4, "row": 3, "all": DEFAULT_ALIAS}]
        level["walls"] = [{"c0": 2, "r0": 5, "c1": 3, "r1": 5, "all": DEFAULT_ALIAS}]
        data["ramps"] = [
            {"lower_level": 0, "low": [col, 1], "high": [col + 2, 2], "all": DEFAULT_ALIAS} for col in (1, 4)
        ]
        window.doc.replace_with_new(data)
        window.set_level_index(1)
        before = copy.deepcopy(window.map_data)
        material = next(alias for alias in window.materials_catalog if alias != DEFAULT_ALIAS)
        for name, point, title in (
            ("floors", (2.5, 3.5), "Edit Floor Materials..."),
            ("inaccessible_floors", (4.5, 3.5), "Edit Blocked Floor Materials..."),
            ("walls", (2.5, 5.0), "Edit Wall Materials..."),
            ("ramps", (1.5, 1.5), "Edit Ramp Materials..."),
        ):
            with self.subTest(name=name):
                history = window.undo_stack.count()
                with patch("map_editor.placement.MaterialAssignmentDialog.prompt", return_value=None):
                    self.choose_action(point, title)
                self.assertEqual(window.map_data, before)
                self.assertEqual(window.undo_stack.count(), history)
                with patch(
                    "map_editor.placement.MaterialAssignmentDialog.prompt", return_value={"top": material}
                ) as prompt:
                    self.choose_action(point, title)
                self.assertEqual(prompt.call_args.args[4]["top"], DEFAULT_ALIAS)
                expected = copy.deepcopy(before)
                target = expected if name == "ramps" else expected["levels"][1]
                target[name][0]["top"] = material
                self.assertEqual(window.map_data, expected)
                window.undo_stack.undo()
                self.assertEqual(window.map_data, before)
                window.undo_stack.redo()
                self.assertEqual(window.map_data, expected)
                window.undo_stack.undo()

    def test_ladder_span_edit_from_an_upper_level_keeps_its_base_and_undoes(self):
        window = self.window
        data = empty_map(8, 8)
        data["levels"] = [empty_level(i) for i in range(5)]
        data["ladders"] = [{"lower_level": 1, "col": 2, "row": 3, "side": "N", "levels": 2}]
        window.doc.replace_with_new(data)
        window.set_level_index(3)
        before = copy.deepcopy(window.map_data)
        for result in ((1, False), (2, True)):
            with patch("map_editor.ladders.QInputDialog.getInt", return_value=result):
                self.choose_action((2.5, 3.0), "Edit Ladder...")
            self.assertEqual(window.map_data, before)
            self.assertEqual(window.undo_stack.count(), 0)
        with patch("map_editor.ladders.QInputDialog.getInt", return_value=(3, True)) as prompt:
            self.choose_action((2.5, 3.0), "Edit Ladder...")
        self.assertEqual(prompt.call_args.args[3:], (2, 1, 3))
        expected = copy.deepcopy(before)
        expected["ladders"][0]["levels"] = 3
        self.assertEqual(window.map_data, expected)
        window.undo_stack.undo()
        self.assertEqual(window.map_data, before)
        window.undo_stack.redo()
        self.assertEqual(window.map_data, expected)

    def test_ladder_extension_cannot_overlap_another_ladder(self):
        window = self.window
        data = empty_map(8, 8)
        data["levels"] = [empty_level(i) for i in range(4)]
        data["ladders"] = [
            {"lower_level": 0, "col": 2, "row": 3, "side": "N", "levels": 1},
            {"lower_level": 2, "col": 2, "row": 3, "side": "N", "levels": 1},
        ]
        window.doc.replace_with_new(data)
        before = copy.deepcopy(window.map_data)
        with (
            patch("map_editor.ladders.QInputDialog.getInt", return_value=(3, True)),
            patch.object(window, "notify") as notify,
        ):
            self.choose_action((2.5, 3.0), "Edit Ladder...")
        self.assertIn("already spans", notify.call_args.args[0])
        self.assertEqual(window.map_data, before)
        self.assertEqual(window.undo_stack.count(), 0)
