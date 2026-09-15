import copy
from unittest.mock import patch

from PySide6.QtCore import QPointF
from PySide6.QtGui import QContextMenuEvent
from PySide6.QtWidgets import QMenu

from editor_fixtures import DEFAULT_ALIAS, WindowTestCase, floor
from map_editor.constants import MODE_ERASE_KEEP_FLOORS, MODE_LIGHT_BRIDGE, MODE_SELECT
from map_editor.normalization import empty_level, empty_map
from map_editor.elements import ElementRef


class ContextEditingTests(WindowTestCase):
    def context(self, point, title=None):
        canvas = self.window.canvas
        position = canvas.viewport.from_grid(QPointF(*point)).toPoint()
        menu = QMenu(canvas)

        def choose(*_):
            if title is not None:
                action = next((a for a in menu.actions() if a.text().replace("&", "") == title), None)
                self.assertIsNotNone(action, [a.text() for a in menu.actions()])
                action.trigger()

        menu.exec = choose
        with patch("map_editor.interaction.QMenu", return_value=menu):
            canvas.contextMenuEvent(
                QContextMenuEvent(QContextMenuEvent.Reason.Mouse, position, canvas.mapToGlobal(position))
            )
        actions = [a.text().replace("&", "") for a in menu.actions()]
        menu.deleteLater()
        return actions

    def test_kind_edits_target_one_record_in_the_viewed_map_and_level(self):
        window = self.window
        window.bridge_kind_colors = window.barrier_kind_colors = {"a": "#ff0000", "b": "#0000ff"}
        window.wall_light_kinds = ["a", "b"]
        cases = (
            ("light_bridges", {"col": 2, "row": 3, "kind": "a"}, (2.5, 3.5), "Edit…"),
            ("barriers", {"c0": 2, "r0": 3, "c1": 3, "r1": 3, "kind": "a"}, (2.5, 3.0), "Edit…"),
            ("lights", {"col": 2, "row": 3, "side": "N", "kind": "a"}, (2.5, 3.05), "Edit…"),
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
                        self.context(point)
                        self.assertEqual(window.properties_panel.widgets[("kind",)].currentData(), "a")
                        self.set_property("kind", "b")
                        window.properties_panel.apply_button.click()
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
                ((2.5, 3.5), "Edit…"),
                ((2.5, 4.0), "Edit…"),
                ((4.5, 3.05), "Edit…"),
            ):
                with self.subTest(choice=choice, title=title):
                    self.context(point)
                    if choice is None:
                        window.properties_panel.rebuild()
                    else:
                        window.properties_panel.apply()
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
            ("floors", (2.5, 3.5), "Edit…"),
            ("inaccessible_floors", (4.5, 3.5), "Edit…"),
            ("walls", (2.5, 5.0), "Edit…"),
            ("ramps", (1.5, 1.5), "Edit…"),
        ):
            with self.subTest(name=name):
                history = window.undo_stack.count()
                self.context(point)
                self.assertEqual(window.map_data, before)
                self.assertEqual(window.undo_stack.count(), history)
                self.assertEqual(window.properties_panel.widgets[("top",)].currentData(), DEFAULT_ALIAS)
                self.set_property("top", material)
                window.properties_panel.apply_button.click()
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
        self.context((2.5, 3.0))
        self.set_property("levels", 1)
        window.properties_panel.rebuild()
        self.assertEqual(window.map_data, before)
        self.assertEqual(window.undo_stack.count(), 0)
        self.set_property("levels", 2)
        window.properties_panel.apply_button.click()
        self.assertEqual(window.map_data, before)
        self.assertEqual(window.undo_stack.count(), 0)
        self.set_property("levels", 3)
        window.properties_panel.apply_button.click()
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
        self.context((2.5, 3.0))
        self.set_property("levels", 3)
        window.properties_panel.apply_button.click()
        self.assertTrue(window.properties_panel.error.isVisible())
        self.assertIn("overlap", window.properties_panel.error.text())
        self.assertEqual(window.map_data, before)
        self.assertEqual(window.undo_stack.count(), 0)

    def test_context_targets_the_clicked_object_then_keeps_an_existing_group(self):
        window = self.window
        data = empty_map(8, 8)
        data["player_spawn_zones"] = []
        data["levels"][0]["floors"] = [floor(col, 1) for col in (1, 3, 5)]
        data["items"] = [{"level": 0, "col": col, "row": 1, "type": "gold"} for col in (1, 3, 5)]
        window.doc.replace_with_new(data)
        window.inspect_refs([ElementRef("items", 0)])
        self.context((3.5, 1.5), "Delete")
        self.assertEqual([item["col"] for item in window.map_data["items"]], [1, 5])
        self.assertEqual(window.map_data["levels"], data["levels"])
        window.undo_stack.undo()
        window.inspect_refs([ElementRef("items", 0), ElementRef("items", 1)])
        actions = self.context((3.5, 1.5), "Copy")
        self.assertNotIn("Edit…", actions)
        self.assertNotIn("Use This Tool", actions)
        self.assertEqual(len(window.tile_clipboard["items"]), 2)
        self.assertEqual(window.properties_panel.summary.text(), "2 selected")
        self.context((1.5, 1.5), "Delete")
        self.assertEqual([item["col"] for item in window.map_data["items"]], [5])
        self.assertEqual(window.map_data["levels"], data["levels"])

    def test_empty_context_only_pastes_at_the_clicked_location(self):
        window = self.window
        window.set_tile_selection((1, 1, 2, 2))
        window.copy_selection()
        actions = self.context((5.5, 5.5), "Paste")
        self.assertEqual(actions, ["Paste"])
        self.assertEqual([(f["col"], f["row"]) for f in window.map_data["levels"][0]["floors"]], [(1, 1), (5, 5)])
        self.app.clipboard().clear()
        self.assertEqual(self.context((6.5, 6.5)), [])
        self.assertEqual(window.selection_refs(), [])

    def test_context_inside_a_tile_area_keeps_the_area_as_its_target(self):
        window = self.window
        data = empty_map(8, 8)
        data["player_spawn_zones"] = []
        data["levels"][0]["floors"] = [floor(col, 1) for col in (1, 2, 5)]
        window.doc.replace_with_new(data)
        window.selection_kind_changed("Tiles")
        window.set_tile_selection((1, 1, 3, 2))
        self.context((1.5, 1.5), "Delete")
        self.assertEqual([(f["col"], f["row"]) for f in window.map_data["levels"][0]["floors"]], [(5, 1)])
