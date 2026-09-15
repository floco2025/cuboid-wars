"""End-to-end interaction rules shared by mouse input, Properties and commands."""

import copy
from unittest.mock import patch

from PySide6.QtCore import QPointF, Qt
from PySide6.QtGui import QContextMenuEvent
from PySide6.QtTest import QTest
from PySide6.QtWidgets import QMenu

from editor_fixtures import WindowTestCase, floor, nested
from map_editor import constants as c
from map_editor.elements import ElementRef
from map_editor.normalization import empty_level, empty_map


class EditorInputTests(WindowTestCase):
    def point(self, x, y):
        return self.window.canvas.viewport.from_grid(QPointF(x, y)).toPoint()

    def click_at(self, x, y, modifiers=Qt.KeyboardModifier.NoModifier):
        QTest.mouseClick(self.window.canvas, Qt.MouseButton.LeftButton, modifiers, pos=self.point(x, y))

    def drag(self, start, end):
        canvas = self.window.canvas
        QTest.mousePress(canvas, Qt.MouseButton.LeftButton, pos=self.point(*start))
        QTest.mouseMove(canvas, self.point(*end))
        QTest.mouseRelease(canvas, Qt.MouseButton.LeftButton, pos=self.point(*end))

    def context(self, x, y):
        canvas = self.window.canvas
        position = self.point(x, y)
        menu = QMenu(canvas)
        with patch("map_editor.interaction.QMenu", return_value=menu), patch.object(menu, "exec"):
            canvas.contextMenuEvent(
                QContextMenuEvent(QContextMenuEvent.Reason.Mouse, position, canvas.mapToGlobal(position))
            )
        actions = [action.text().replace("&", "") for action in menu.actions()]
        menu.deleteLater()
        return actions

    def zone_map(self):
        data = empty_map(8, 8)
        data["player_spawn_zones"] = []
        data["actor_spawn_zones"] = [
            {"level": 0, "cols": [2, 4], "rows": [2, 4], "kind": "scuttler", "count": [2], "respawn_secs": 90}
        ]
        self.window.doc.replace_with_new(data)
        return copy.deepcopy(self.window.map_data)

    def test_left_and_right_click_publish_identical_selection_properties_and_handles(self):
        self.zone_map()
        window = self.window
        self.click_at(3, 3)
        selected = window.selection
        self.assertEqual(window.selection_refs(), [ElementRef("actor_spawn_zones", 0)])
        self.assertIsNone(selected.area)
        self.assertEqual(len(window.canvas.input.handles()), 8)
        self.assertEqual(window.properties_panel.widgets[("count",)].text(), "2")
        window.clear_selection()
        window.selection_kind_changed("Tiles")
        actions = self.context(3, 3)
        self.assertEqual(window.selection, selected)
        self.assertEqual(window.selection_kind, "Objects")
        self.assertEqual(len(window.canvas.input.handles()), 8)
        self.assertNotIn("Edit…", actions)
        window.set_mode(c.MODE_WALL)
        self.context(3, 3)
        self.assertEqual(window.selection, selected)
        self.assertEqual(window.mode, c.MODE_SELECT)

    def test_resize_handles_take_priority_over_moving_and_remain_selected_after_apply(self):
        before = self.zone_map()
        window = self.window
        self.click_at(3, 3)
        self.drag((2, 2), (1, 1))
        zone = window.map_data["actor_spawn_zones"][0]
        self.assertEqual((zone["cols"], zone["rows"]), ([1, 4], [1, 4]))
        self.assertEqual(window.selection_refs(), [ElementRef("actor_spawn_zones", 0)])
        self.assertEqual(len(window.canvas.input.handles()), 8)
        self.assertEqual(window.undo_stack.count(), 1)
        window.undo_stack.undo()
        self.assertEqual(window.map_data, before)
        self.click_at(3, 3)
        self.drag((3, 3), (5, 3))
        self.assertEqual(window.map_data["actor_spawn_zones"][0]["cols"], [4, 6])
        self.assertEqual(window.map_data["actor_spawn_zones"][0]["rows"], [2, 4])

    def test_nested_handle_moves_one_end_without_moving_the_other(self):
        data = empty_map(8, 8)
        data["player_spawn_zones"] = []
        child = empty_map(2, 2)
        child["player_spawn_zones"] = []
        data["nested_geometry"] = {"cabin": child}
        data["nested_maps"] = [nested("cabin", 0, [1, 1], [5, 1])]
        self.window.doc.replace_with_new(data)
        self.click_at(5.5, 1.5)
        self.drag((5.5, 1.5), (5.5, 4.5))
        entry = self.window.map_data["nested_maps"][0]
        self.assertEqual((entry["from"], entry["to"]), ([1, 1], [5, 4]))
        self.assertEqual(len(self.window.canvas.input.handles()), 2)
        self.window.undo_stack.undo()
        self.assertEqual(self.window.map_data["nested_maps"][0]["to"], [5, 1])

    def test_text_editing_shortcuts_do_not_delete_or_copy_map_objects(self):
        before = self.zone_map()
        window = self.window
        self.click_at(3, 3)
        field = window.properties_panel.widgets[("count",)]
        field.setFocus()
        field.selectAll()
        QTest.keyClick(field, Qt.Key.Key_Delete)
        self.assertEqual(field.text(), "")
        self.assertEqual(window.map_data, before)
        QTest.keyClicks(field, "3")
        field.selectAll()
        QTest.keyClick(field, Qt.Key.Key_C, Qt.KeyboardModifier.ControlModifier)
        self.assertEqual(self.app.clipboard().text(), "3")
        self.assertIsNone(window.tile_clipboard)
        self.assertEqual(window.map_data, before)

    def test_shift_selection_and_plain_click_keep_one_group_for_every_command(self):
        data = empty_map(8, 8)
        data["player_spawn_zones"] = []
        data["levels"][0]["floors"] = [floor(col, 1) for col in (1, 3, 5)]
        data["items"] = [{"level": 0, "col": col, "row": 1, "type": "gold"} for col in (1, 3, 5)]
        window = self.window
        window.doc.replace_with_new(data)
        self.click_at(1.5, 1.5)
        self.click_at(3.5, 1.5, Qt.KeyboardModifier.ShiftModifier)
        selection = window.selection
        self.click_at(1.5, 1.5)
        self.assertEqual(window.selection, selection)
        self.context(3.5, 1.5)
        self.assertEqual(window.selection, selection)
        window.copy_selection()
        self.assertEqual(len(window.tile_clipboard["items"]), 2)
        window.delete_selection()
        self.assertEqual([item["col"] for item in window.map_data["items"]], [5])
        self.assertEqual(window.map_data["levels"], data["levels"])

    def test_shift_drag_selects_a_group_starting_on_an_occupied_tile(self):
        data = empty_map(8, 8)
        data["player_spawn_zones"] = []
        data["levels"][0]["floors"] = [floor(col, row) for col in range(8) for row in range(8)]
        data["items"] = [{"level": 0, "col": col, "row": 1, "type": "gold"} for col in (1, 3, 5)]
        window = self.window
        window.doc.replace_with_new(data)
        before = copy.deepcopy(window.map_data)
        self.click_at(5.5, 1.5)
        QTest.mousePress(
            window.canvas, Qt.MouseButton.LeftButton, Qt.KeyboardModifier.ShiftModifier, pos=self.point(1.5, 1.5)
        )
        QTest.mouseRelease(
            window.canvas, Qt.MouseButton.LeftButton, Qt.KeyboardModifier.ShiftModifier, pos=self.point(3.5, 1.5)
        )
        refs = window.selection_refs()
        self.assertEqual({ref.get(window.map_data)["col"] for ref in refs if ref.name == "items"}, {1, 3, 5})
        self.assertFalse(any(ref.name == "floors" for ref in refs))
        self.assertIsNone(window.selection.area)
        self.assertEqual(window.map_data, before)
        self.assertIsNone(window.pending_block)

    def test_plain_and_shift_drag_select_only_the_starting_edge_type_over_floors(self):
        for name in ("walls", "erasers"):
            for boundary in (False, True):
                for additive in (False, True):
                    with self.subTest(name=name, boundary=boundary, additive=additive):
                        window = self.window
                        data = empty_map(8, 8)
                        data["player_spawn_zones"] = []
                        data["levels"][0]["floors"] = [floor(col, row) for row in range(8) for col in range(8)]
                        col = 8 if boundary else 2
                        for kind, x in ((name, col), ("erasers" if name == "walls" else "walls", col - 1)):
                            data["levels"][0][kind] = [
                                {
                                    "c0": x,
                                    "r0": row,
                                    "c1": x,
                                    "r1": row + 1,
                                    **({"all": "basement-floor"} if kind == "walls" else {}),
                                }
                                for row in (1, 2, 3, 6)
                            ]
                        window.doc.replace_with_new(data)
                        before = copy.deepcopy(window.map_data)
                        if additive:
                            self.click_at(col, 6.5)
                        modifiers = Qt.KeyboardModifier.ShiftModifier if additive else Qt.KeyboardModifier.NoModifier
                        start, end = (col, 1.5), (col - 0.5, 3.5)
                        if boundary:
                            start, end = (col, 3.5), (col - 0.5, 1.5)
                        QTest.mousePress(window.canvas, Qt.MouseButton.LeftButton, modifiers, pos=self.point(*start))
                        QTest.mouseRelease(window.canvas, Qt.MouseButton.LeftButton, modifiers, pos=self.point(*end))
                        refs = window.selection_refs()
                        self.assertEqual({ref.name for ref in refs}, {name})
                        self.assertEqual(
                            {ref.get(window.map_data)["r0"] for ref in refs}, {1, 2, 3, 6} if additive else {1, 2, 3}
                        )
                        self.assertIsNone(window.selection.area)
                        self.assertEqual(window.map_data, before)
                        window.delete_selection()
                        self.assertEqual(window.map_data["levels"][0]["floors"], before["levels"][0]["floors"])
                        other = "erasers" if name == "walls" else "walls"
                        self.assertEqual(window.map_data["levels"][0][other], before["levels"][0][other])
                        self.assertEqual(
                            [edge["r0"] for edge in window.map_data["levels"][0][name]], [] if additive else [6]
                        )
                        window.undo_stack.undo()
                        self.assertEqual(window.map_data, before)

    def test_line_drag_starting_before_edges_selects_only_that_line_and_previews_it(self):
        for name in ("walls", "erasers"):
            for vertical in (False, True):
                for reverse in (False, True):
                    with self.subTest(name=name, vertical=vertical, reverse=reverse):
                        window = self.window
                        data = empty_map(8, 8)
                        data["player_spawn_zones"] = []
                        data["levels"][0]["floors"] = [floor(col, row) for row in range(8) for col in range(8)]

                        def edge(x0, y0, x1, y1):
                            if not vertical:
                                x0, y0, x1, y1 = y0, x0, y1, x1
                            return {
                                "c0": x0,
                                "r0": y0,
                                "c1": x1,
                                "r1": y1,
                                **({"all": "basement-floor"} if name == "walls" else {}),
                            }

                        wanted = [edge(2, row, 2, row + 1) for row in (1, 2, 3)]
                        data["levels"][0][name] = (
                            wanted + [edge(3, row, 3, row + 1) for row in (1, 2, 3)] + [edge(1, 2, 3, 2)]
                        )
                        window.doc.replace_with_new(data)
                        before = copy.deepcopy(window.map_data)
                        canvas = window.canvas
                        canvas.viewport.fitted = False
                        canvas.viewport.cell = 48 if reverse else 32
                        canvas.viewport.offset = QPointF(30, 30)
                        offset = 4 / canvas.cell_size()
                        start, end = (2 - offset, 0.5), (2 + offset, 4.5)
                        if not vertical:
                            start, end = start[::-1], end[::-1]
                        if reverse:
                            start, end = end, start
                        self.assertEqual(canvas.input.hits(QPointF(*start))[0].name, "floors")
                        QTest.mousePress(canvas, Qt.MouseButton.LeftButton, pos=self.point(*start))
                        QTest.mouseMove(canvas, self.point(*end))
                        refs = window.selection_refs()
                        self.assertEqual({ref.name for ref in refs}, {name})
                        selected = {
                            tuple(ref.get(window.map_data)[key] for key in ("c0", "r0", "c1", "r1")) for ref in refs
                        }
                        self.assertEqual(
                            selected, {tuple(entry[key] for key in ("c0", "r0", "c1", "r1")) for entry in wanted}
                        )
                        self.assertEqual(window.properties_panel.refs, refs)
                        self.assertEqual(window.properties_panel.summary.text(), "3 selected")
                        line = canvas.input.line_selection()[0]
                        self.assertEqual(line[0], line[2]) if vertical else self.assertEqual(line[1], line[3])
                        self.assertIsNone(window.selection.area)
                        self.assertIsNone(window.pending_block)
                        self.assertEqual(window.map_data, before)
                        QTest.mouseRelease(canvas, Qt.MouseButton.LeftButton, pos=self.point(*end))
                        self.assertEqual(window.selection_refs(), refs)
                        window.copy_selection()
                        self.assertEqual(window.tile_clipboard["levels"][0]["floors"], [])
                        window.delete_selection()
                        self.assertEqual(window.map_data["levels"][0]["floors"], before["levels"][0]["floors"])
                        self.assertEqual(len(window.map_data["levels"][0][name]), 4)
                        window.undo_stack.undo()
                        self.assertEqual(window.map_data, before)

    def test_line_selection_excludes_adjacent_segments_and_preserves_tiles_scope(self):
        window = self.window
        data = empty_map(8, 8)
        data["player_spawn_zones"] = []
        data["levels"][0]["floors"] = [floor(col, row) for row in range(8) for col in range(8)]
        data["levels"][0]["walls"] = [
            {"c0": 2, "r0": row, "c1": 2, "r1": row + 1, "all": "basement-floor"} for row in range(6)
        ]
        window.doc.replace_with_new(data)
        canvas = window.canvas
        canvas.viewport.fitted = False
        canvas.viewport.cell = 48
        canvas.viewport.offset = QPointF(30, 30)
        self.drag((2, 1), (2, 4))
        self.assertEqual([ref.get(window.map_data)["r0"] for ref in window.selection_refs()], [1, 2, 3])
        window.selection_kind_changed("Tiles")
        self.drag((2, 1), (2, 4))
        self.assertEqual(window.selection.area.rect, (2, 1, 3, 5))
        self.assertTrue(any(ref.name == "floors" for ref in window.selection_refs()))

    def test_edge_line_uses_the_first_collinear_type_in_the_drag_direction(self):
        for start, end, name in (
            ((2, 0.5), (2, 5.5), "walls"),
            ((2, 5.5), (2, 0.5), "erasers"),
            ((2, 3), (2, 5.5), "erasers"),
        ):
            with self.subTest(start=start):
                window = self.window
                data = empty_map(8, 8)
                data["player_spawn_zones"] = []
                data["levels"][0]["floors"] = [floor(col, row) for row in range(8) for col in range(8)]
                data["levels"][0]["walls"] = [
                    {"c0": 2, "r0": row, "c1": 2, "r1": row + 1, "all": "basement-floor"} for row in (1, 2)
                ]
                data["levels"][0]["walls"].append({"c0": 1, "r0": 3, "c1": 3, "r1": 3, "all": "basement-floor"})
                data["levels"][0]["erasers"] = [{"c0": 2, "r0": row, "c1": 2, "r1": row + 1} for row in (3, 4)]
                window.doc.replace_with_new(data)
                window.canvas.viewport.fitted = False
                window.canvas.viewport.cell = 48
                window.canvas.viewport.offset = QPointF(30, 30)
                self.drag(start, end)
                self.assertEqual({ref.name for ref in window.selection_refs()}, {name})
                self.assertEqual(len(window.selection_refs()), 2)

    def test_floor_drag_along_an_empty_grid_edge_still_selects_floors(self):
        window = self.window
        data = empty_map(8, 8)
        data["player_spawn_zones"] = []
        data["levels"][0]["floors"] = [floor(col, row) for row in range(8) for col in range(8)]
        window.doc.replace_with_new(data)
        before = copy.deepcopy(window.map_data)
        self.drag((2, 0.5), (2, 4.5))
        self.assertEqual({ref.name for ref in window.selection_refs()}, {"floors"})
        self.assertEqual(len(window.selection_refs()), 5)
        self.assertEqual(window.map_data, before)

    def test_dragging_an_already_selected_wall_moves_it_without_selecting_floors(self):
        window = self.window
        data = empty_map(8, 8)
        data["player_spawn_zones"] = []
        data["levels"][0]["floors"] = [floor(col, row) for row in range(8) for col in range(8)]
        data["levels"][0]["walls"] = [{"c0": 2, "r0": 1, "c1": 2, "r1": 2, "all": "basement-floor"}]
        window.doc.replace_with_new(data)
        before = copy.deepcopy(window.map_data)
        self.click_at(2, 1.5)
        self.drag((2, 1.5), (4, 1.5))
        edge = window.map_data["levels"][0]["walls"][0]
        self.assertEqual((edge["c0"], edge["c1"]), (4, 4))
        self.assertEqual(window.map_data["levels"][0]["floors"], before["levels"][0]["floors"])
        self.assertEqual({ref.name for ref in window.selection_refs()}, {"walls"})
        window.undo_stack.undo()
        self.assertEqual(window.map_data, before)

    def test_first_floor_drag_selects_and_only_a_later_drag_moves_the_selection(self):
        window = self.window
        data = empty_map(8, 8)
        data["player_spawn_zones"] = []
        data["levels"][0]["floors"] = [floor(col, row) for row in (1, 2) for col in (1, 2)] + [floor(7, 7)]
        window.doc.replace_with_new(data)
        before = copy.deepcopy(window.map_data)
        QTest.mousePress(window.canvas, Qt.MouseButton.LeftButton, pos=self.point(1.5, 1.5))
        QTest.mouseMove(window.canvas, self.point(2.5, 2.5))
        self.assertIsNone(window.pending_block)
        self.assertEqual(window.map_data, before)
        QTest.keyClick(window.canvas, Qt.Key.Key_Escape)
        QTest.mouseRelease(window.canvas, Qt.MouseButton.LeftButton, pos=self.point(2.5, 2.5))
        self.assertTrue(window.selection.empty)
        self.drag((1.5, 1.5), (2.5, 2.5))
        self.assertEqual(window.map_data, before)
        self.assertEqual(window.undo_stack.count(), 0)
        self.assertEqual(len(window.selection_refs()), 4)
        self.assertEqual({ref.name for ref in window.selection_refs()}, {"floors"})
        self.drag((1.5, 1.5), (4.5, 1.5))
        self.assertEqual(
            {(entry["col"], entry["row"]) for entry in window.map_data["levels"][0]["floors"]},
            {(4, 1), (5, 1), (4, 2), (5, 2), (7, 7)},
        )
        self.assertEqual(window.undo_stack.count(), 1)
        window.undo_stack.undo()
        self.assertEqual(window.map_data, before)

    def test_escape_cancels_resize_and_move_before_clearing_selection(self):
        before = self.zone_map()
        window = self.window
        self.click_at(3, 3)
        for start, end in (((2, 2), (1, 1)), ((3, 3), (5, 3))):
            with self.subTest(start=start):
                selected = window.selection
                QTest.mousePress(window.canvas, Qt.MouseButton.LeftButton, pos=self.point(*start))
                QTest.mouseMove(window.canvas, self.point(*end))
                QTest.keyClick(window.canvas, Qt.Key.Key_Escape)
                QTest.mouseRelease(window.canvas, Qt.MouseButton.LeftButton, pos=self.point(*end))
                self.assertEqual(window.map_data, before)
                self.assertEqual(window.selection, selected)
                self.assertIsNone(window.pending_block)
        QTest.keyClick(window.canvas, Qt.Key.Key_Escape)
        self.assertTrue(window.selection.empty)
        self.assertFalse(window.delete_action.isEnabled())

    def test_ladder_selection_highlights_its_rails_instead_of_the_anchor_tile(self):
        for side, click in (("N", (3.5, 3)), ("S", (3.5, 4)), ("W", (3, 3.5)), ("E", (4, 3.5))):
            with self.subTest(side=side):
                window = self.window
                data = empty_map(8, 8)
                data["player_spawn_zones"] = []
                data["levels"].append(empty_level(1))
                data["levels"][0]["floors"] = [floor(col, row) for row in range(8) for col in range(8)]
                data["ladders"] = [{"col": 3, "row": 3, "side": side, "lower_level": 0, "levels": 1}]
                window.doc.replace_with_new(data)
                canvas = window.canvas
                canvas.viewport.fitted = False
                canvas.viewport.cell = 48
                canvas.viewport.offset = QPointF(30, 30)
                before = canvas.grab().toImage()
                self.click_at(*click)
                self.assertEqual(window.selection_refs(), [ElementRef("ladders", 0)])
                after = canvas.grab().toImage()
                low, high = self.point(2.5, 2.5), self.point(4.5, 4.5)
                changed = [
                    (x, y)
                    for y in range(low.y(), high.y())
                    for x in range(low.x(), high.x())
                    if before.pixel(x, y) != after.pixel(x, y)
                ]
                self.assertTrue(changed)
                top_left, bottom_right = self.point(3, 3), self.point(4, 4)
                outside_anchor = {
                    "N": lambda x, y: y < top_left.y(),
                    "S": lambda x, y: y >= bottom_right.y(),
                    "W": lambda x, y: x < top_left.x(),
                    "E": lambda x, y: x >= bottom_right.x(),
                }[side]
                self.assertTrue(all(outside_anchor(x, y) for x, y in changed))
                self.assertIsNone(window.selection.area)

    def test_item_selection_highlight_does_not_fill_its_floor_tile(self):
        data = empty_map(8, 8)
        data["player_spawn_zones"] = []
        data["levels"][0]["floors"] = [floor(2, 2)]
        data["items"] = [{"level": 0, "col": 2, "row": 2, "type": "gold"}]
        window = self.window
        window.doc.replace_with_new(data)
        corner = self.point(2.12, 2.12)
        before = window.canvas.grab().toImage().pixelColor(corner)
        self.click_at(2.5, 2.5)
        self.assertIsNone(window.selection.area)
        self.assertEqual(window.canvas.grab().toImage().pixelColor(corner), before)
        self.context(2.5, 2.5)
        self.assertEqual(window.canvas.grab().toImage().pixelColor(corner), before)

    def test_wall_tool_can_start_and_end_on_the_map_boundary(self):
        window = self.window
        window.set_mode(c.MODE_WALL)
        self.drag((8, 1), (8, 4))
        walls = window.map_data["levels"][0]["walls"]
        self.assertEqual(
            [(wall["c0"], wall["r0"], wall["c1"], wall["r1"]) for wall in walls],
            [(8, 1, 8, 2), (8, 2, 8, 3), (8, 3, 8, 4)],
        )
        window.undo_stack.undo()
        self.assertEqual(window.map_data["levels"][0]["walls"], [])

    def test_focus_loss_cancels_an_active_move_and_its_preview(self):
        before = self.zone_map()
        window = self.window
        self.click_at(3, 3)
        QTest.mousePress(window.canvas, Qt.MouseButton.LeftButton, pos=self.point(3, 3))
        QTest.mouseMove(window.canvas, self.point(5, 3))
        self.assertIsNotNone(window.pending_block)
        window.properties_panel.widgets[("count",)].setFocus()
        self.app.processEvents()
        QTest.mouseRelease(window.canvas, Qt.MouseButton.LeftButton, pos=self.point(5, 3))
        self.assertIsNone(window.pending_block)
        self.assertIsNone(window.canvas.input.gesture)
        self.assertEqual(window.map_data, before)

    def test_placement_tools_do_not_erase_ladders_or_move_existing_nested_maps(self):
        window = self.window
        data = empty_map(8, 8)
        data["player_spawn_zones"] = []
        data["levels"].append(empty_level(1))
        data["ladders"] = [{"col": 2, "row": 2, "side": "S", "lower_level": 0, "levels": 1}]
        child = empty_map(1, 1)
        child["player_spawn_zones"] = []
        data["nested_geometry"] = {"cabin": child}
        data["nested_maps"] = [nested("cabin", 0, [4, 4], [5, 4])]
        window.doc.replace_with_new(data)
        before = copy.deepcopy(window.map_data)
        window.set_mode(c.MODE_LADDER)
        self.click_at(2.5, 3.05)
        self.assertEqual(window.map_data, before)
        window.set_mode(c.MODE_NESTED_MAP)
        self.drag((4.5, 4.5), (6.5, 4.5))
        self.assertEqual(window.map_data, before)
        self.assertTrue(window.selection.empty)
        window.set_mode(c.MODE_ERASE_LADDERS)
        self.drag((2.5, 2.5), (2.5, 3.5))
        self.assertEqual(window.map_data["ladders"], [])
        window.undo_stack.undo()
        self.assertEqual(window.map_data, before)
