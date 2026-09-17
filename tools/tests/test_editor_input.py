"""End-to-end interaction rules shared by mouse input, Properties and commands."""

import copy
from unittest.mock import patch

from PySide6.QtCore import QEvent, QPointF, Qt
from PySide6.QtGui import QContextMenuEvent, QFocusEvent
from PySide6.QtTest import QTest
from PySide6.QtWidgets import QMenu

from editor_fixtures import DEFAULT_ALIAS, WindowTestCase, floor, nested
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
        data["checkpoints"] = []
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
        data["checkpoints"] = []
        child = empty_map(2, 2)
        child["checkpoints"] = []
        data["nested_geometry"] = {"cabin": child}
        data["nested_maps"] = [nested("cabin", 0, [1, 1], [5, 1])]
        self.window.doc.replace_with_new(data)
        self.click_at(5.5, 1.5)
        canvas = self.window.canvas
        edge = self.point(7, 4.75)
        edge.setX(edge.x() - 2)
        before = canvas.grab().toImage().pixelColor(edge)
        QTest.mousePress(canvas, Qt.MouseButton.LeftButton, pos=self.point(5.5, 1.5))
        QTest.mouseMove(canvas, self.point(5.5, 4.5))
        self.assertEqual(self.window.map_data["nested_maps"][0]["to"], [5, 1])
        self.assertNotEqual(canvas.grab().toImage().pixelColor(edge), before)
        QTest.mouseRelease(canvas, Qt.MouseButton.LeftButton, pos=self.point(5.5, 4.5))
        entry = self.window.map_data["nested_maps"][0]
        self.assertEqual((entry["from"], entry["to"]), ([1, 1], [5, 4]))
        self.assertEqual(len(self.window.canvas.input.handles()), 2)
        self.window.undo_stack.undo()
        self.assertEqual(self.window.map_data["nested_maps"][0]["to"], [5, 1])

    def test_dragging_a_platform_label_or_outline_moves_its_full_footprints(self):
        window = self.window
        data = empty_map(12, 12)
        data["checkpoints"] = []
        data["levels"][0]["floors"] = [floor(3, 1)]
        child = empty_map(3, 2)
        child["checkpoints"] = []
        data["nested_geometry"] = {"platform": child}
        data["nested_maps"] = [{**nested("platform", 0, [1, 1], [6, 1]), "from_nudge": [1, 0, 0]}]
        window.doc.replace_with_new(data)
        before = copy.deepcopy(window.map_data)
        canvas = window.canvas
        self.assertEqual(canvas.input.hits(QPointF(3.4, 1.5)), [ElementRef("floors", 0, 0)])
        nudge = window.wall_width_cells
        for start in ((2.5 + nudge, 2), (4 + nudge, 1.75)):
            with self.subTest(start=start):
                window.clear_selection()
                end = (start[0], start[1] + 3)
                edge = self.point(4 + nudge, 4.75)
                edge.setX(edge.x() - 2)
                original_pixel = canvas.grab().toImage().pixelColor(edge)
                QTest.mousePress(canvas, Qt.MouseButton.LeftButton, pos=self.point(*start))
                QTest.mouseMove(canvas, self.point(*end))
                self.assertEqual(canvas.input.gesture.kind, "move")
                self.assertIsNotNone(window.pending_block)
                self.assertEqual(window.map_data, before)
                self.assertNotEqual(canvas.grab().toImage().pixelColor(edge), original_pixel)
                QTest.keyClick(canvas, Qt.Key.Key_Escape)
                QTest.mouseRelease(canvas, Qt.MouseButton.LeftButton, pos=self.point(*end))
                self.assertIsNone(window.pending_block)
                self.assertEqual(window.map_data, before)
                self.assertEqual(window.undo_stack.count(), 0)
                window.clear_selection()
                self.drag(start, end)
                entry = window.map_data["nested_maps"][0]
                self.assertEqual((entry["from"], entry["to"]), ([1, 4], [6, 4]))
                self.assertEqual(entry["from_nudge"], [1, 0, 0])
                self.assertEqual(window.map_data["levels"], before["levels"])
                self.assertEqual(window.undo_stack.count(), 1)
                window.undo_stack.undo()
                self.assertEqual(window.map_data, before)
                window.undo_stack.clear()

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
        data["checkpoints"] = []
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
        data["checkpoints"] = []
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
                        data["checkpoints"] = []
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
                        data["checkpoints"] = []
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
        data["checkpoints"] = []
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
                data["checkpoints"] = []
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
        data["checkpoints"] = []
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
        data["checkpoints"] = []
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
        data["checkpoints"] = []
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
                data["checkpoints"] = []
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
        data["checkpoints"] = []
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
        data["checkpoints"] = []
        data["levels"].append(empty_level(1))
        data["ladders"] = [{"col": 2, "row": 2, "side": "S", "lower_level": 0, "levels": 1}]
        child = empty_map(1, 1)
        child["checkpoints"] = []
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
        with patch.object(window, "notify") as notify:
            self.drag((5.5, 4.5), (5.5, 6.5))
        self.assertEqual(window.map_data, before)
        self.assertIn("ends here", notify.call_args.args[0])
        window.set_mode(c.MODE_ERASE_LADDERS)
        self.drag((2.5, 2.5), (2.5, 3.5))
        self.assertEqual(window.map_data["ladders"], [])
        window.undo_stack.undo()
        self.assertEqual(window.map_data, before)

    def test_range_tools_place_up_to_the_grid_edge_when_released_past_it(self):
        window = self.window
        window.set_mode(c.MODE_FLOOR)
        self.drag((1.5, 3.5), (8.3, 3.5))
        floors = window.map_data["levels"][0]["floors"]
        self.assertEqual(sorted(f["col"] for f in floors if f["row"] == 3), list(range(1, 8)))
        window.set_mode(c.MODE_WALL)
        self.drag((1, 5), (8.4, 5))
        walls = window.map_data["levels"][0]["walls"]
        self.assertEqual(sorted((w["c0"], w["c1"]) for w in walls), [(col, col + 1) for col in range(1, 8)])

    def test_right_click_keeps_a_pending_duplicate_for_its_transform_menu(self):
        window = self.window
        data = empty_map(8, 8)
        data["checkpoints"] = []
        data["levels"][0]["floors"] = [floor(1, 1), floor(2, 2)]
        data["items"] = [{"level": 0, "col": 1, "row": 1, "type": "gold"}]
        window.doc.replace_with_new(data)
        self.click_at(1.5, 1.5)
        self.assertEqual(window.selection_refs(), [ElementRef("items", 0)])
        window.duplicate_selection()
        actions = self.context(2.5, 2.5)
        self.assertEqual(
            actions, ["Rotate Selection Clockwise", "Mirror Selection Horizontally", "Mirror Selection Vertically"]
        )
        self.assertTrue(window.pending_block.duplicate)
        self.assertEqual(window.selection_refs(), [ElementRef("items", 0)])
        window.transform_selection("rotate")
        self.assertTrue(window.pending_block.duplicate)

    def test_shift_drag_adds_the_box_without_toggling_the_pressed_object(self):
        window = self.window
        data = empty_map(8, 8)
        data["checkpoints"] = []
        data["levels"][0]["floors"] = [floor(col, row) for col in (1, 2) for row in (1, 2)]
        window.doc.replace_with_new(data)
        self.drag((0.5, 0.5), (2.7, 2.7))
        self.assertEqual(len(window.selection_refs()), 4)
        canvas = window.canvas
        shift = Qt.KeyboardModifier.ShiftModifier
        QTest.mousePress(canvas, Qt.MouseButton.LeftButton, shift, pos=self.point(1.5, 1.5))
        self.assertEqual(len(window.selection_refs()), 4)
        QTest.mouseMove(canvas, self.point(1.95, 1.95))
        QTest.mouseRelease(canvas, Qt.MouseButton.LeftButton, shift, pos=self.point(1.95, 1.95))
        self.assertEqual(len(window.selection_refs()), 4)
        self.click_at(1.5, 1.5, shift)
        self.assertEqual(len(window.selection_refs()), 3)
        self.click_at(1.5, 1.5, shift)
        self.assertEqual(len(window.selection_refs()), 4)

    def test_focus_loss_restores_the_selection_a_press_replaced(self):
        window = self.window
        data = empty_map(8, 8)
        data["checkpoints"] = []
        data["levels"][0]["floors"] = [floor(col, row) for col in range(8) for row in range(8)]
        data["items"] = [{"level": 0, "col": col, "row": 1, "type": "gold"} for col in (1, 3, 5)]
        window.doc.replace_with_new(data)
        canvas = window.canvas
        items = [ElementRef("items", index) for index in range(3)]
        window.inspect_refs(items)
        QTest.mousePress(canvas, Qt.MouseButton.LeftButton, pos=self.point(2.5, 2.5))
        self.assertEqual([ref.name for ref in window.selection_refs()], ["floors"])
        canvas.focusOutEvent(QFocusEvent(QEvent.Type.FocusOut))
        self.assertEqual(window.selection_refs(), items)
        self.assertIsNone(canvas.input.gesture)
        window.selection_kind_changed("Tiles")
        window.set_tile_selection((0, 0, 3, 3))
        QTest.mousePress(canvas, Qt.MouseButton.LeftButton, pos=self.point(5.5, 5.5))
        self.assertEqual(window.selection.area.rect, (5, 5, 6, 6))
        canvas.focusOutEvent(QFocusEvent(QEvent.Type.FocusOut))
        self.assertEqual(window.selection.area.rect, (0, 0, 3, 3))

    def test_material_tools_leave_the_selection_scope_alone(self):
        window = self.window
        window.selection_kind_changed("Tiles")
        window.set_mode(c.MODE_FLOOR_MATERIAL)
        self.drag((1.2, 1.2), (1.8, 1.8))
        self.assertEqual(window.selection_refs(), [ElementRef("floors", 0, 0)])
        self.assertEqual(window.selection_kind, "Tiles")
        window.set_mode(c.MODE_SELECT)
        self.drag((0.3, 0.3), (2.7, 2.7))
        self.assertEqual(window.selection.area.rect, (0, 0, 3, 3))

    def test_a_sub_cell_drag_on_a_tile_area_neither_moves_nor_complains(self):
        window = self.window
        data = empty_map(8, 8)
        data["checkpoints"] = []
        data["levels"][0]["walls"] = [{"c0": 2, "r0": 1, "c1": 2, "r1": 2, "all": DEFAULT_ALIAS}]
        data["levels"][0]["lights"] = [{"col": 2, "row": 1, "side": "W"}]
        window.doc.replace_with_new(data)
        before = copy.deepcopy(window.map_data)
        window.selection_kind_changed("Tiles")
        window.set_tile_selection((0, 0, 2, 3))
        with patch.object(window, "notify") as notify:
            self.drag((1.5, 1.5), (1.85, 1.85))
        notify.assert_not_called()
        self.assertEqual(window.map_data, before)
        self.assertIsNone(window.pending_block)
        self.assertEqual(window.selection.area.rect, (0, 0, 2, 3))

    def test_moving_an_object_from_an_upper_storey_keeps_the_tile_span(self):
        window = self.window
        data = empty_map(8, 8)
        data["checkpoints"] = []
        data["levels"] += [empty_level(1), empty_level(2)]
        data["actor_spawn_zones"] = [
            {
                "level": 0,
                "levels": 3,
                "cols": [2, 4],
                "rows": [2, 4],
                "kind": "scuttler",
                "count": [2],
                "respawn_secs": 90,
            }
        ]
        window.doc.replace_with_new(data)
        window.select_level(2)
        self.click_at(3, 3)
        self.drag((3, 3), (5, 5))
        zone = window.map_data["actor_spawn_zones"][0]
        self.assertEqual((zone["cols"], zone["rows"], zone["level"]), ([4, 6], [4, 6], 0))
        self.assertEqual(window.selection_levels, 1)

    def test_records_on_a_nested_footprint_outrank_its_outline_and_selected_interior(self):
        window = self.window
        data = empty_map(12, 12)
        data["checkpoints"] = []
        data["levels"][0]["floors"] = [floor(col, row) for col in range(1, 5) for row in range(1, 3)]
        data["levels"][0]["walls"] = [{"c0": 4, "r0": 1, "c1": 4, "r1": 2, "all": DEFAULT_ALIAS}]
        data["items"] = [{"level": 0, "col": 3, "row": 1, "type": "gold"}]
        child = empty_map(3, 2)
        child["checkpoints"] = []
        data["nested_geometry"] = {"room": child}
        data["nested_maps"] = [nested("room", 0, [1, 1], [1, 1])]
        window.doc.replace_with_new(data)
        hits = window.canvas.input.hits
        room, wall, item, inner = (
            ElementRef("nested_maps", 0),
            ElementRef("walls", 0, 0),
            ElementRef("items", 0),
            ElementRef("floors", 3, 0),
        )
        self.assertEqual(hits(QPointF(4.0, 1.5)), [wall])
        self.assertEqual(hits(QPointF(3.5, 1.5)), [item])
        self.assertEqual(hits(QPointF(1.0, 2.5)), [room])
        self.assertEqual(hits(QPointF(2.5, 2.7)), [inner])
        self.click_at(2.5, 2.0)
        self.assertEqual(window.selection_refs(), [room])
        self.assertEqual(hits(QPointF(2.5, 2.7)), [room])
        self.assertEqual(hits(QPointF(2.5, 2.7), additive=True), [inner])
        self.assertEqual(hits(QPointF(4.0, 1.5)), [wall])
        self.click_at(2.5, 2.7, Qt.KeyboardModifier.ShiftModifier)
        self.assertEqual(window.selection_refs(), [room, inner])
        self.click_at(4.0, 1.5)
        self.assertEqual(window.selection_refs(), [wall])

    def test_escape_restores_the_selection_a_nested_map_press_replaced(self):
        window = self.window
        data = empty_map(12, 12)
        data["checkpoints"] = []
        data["levels"][0]["walls"] = [
            {"c0": 8, "r0": row, "c1": 9, "r1": row, "all": DEFAULT_ALIAS} for row in (6, 7, 8)
        ]
        child = empty_map(3, 2)
        child["checkpoints"] = []
        data["nested_geometry"] = {"room": child}
        data["nested_maps"] = [nested("room", 0, [1, 1], [1, 1])]
        window.doc.replace_with_new(data)
        before = copy.deepcopy(window.map_data)
        canvas = window.canvas
        walls = [ElementRef("walls", index, 0) for index in range(3)]
        for end in ((2.5, 2.0), (2.5, 5.0)):
            with self.subTest(end=end):
                window.inspect_refs(walls)
                QTest.mousePress(canvas, Qt.MouseButton.LeftButton, pos=self.point(2.5, 2.0))
                QTest.mouseMove(canvas, self.point(*end))
                self.assertEqual(window.selection_refs(), [ElementRef("nested_maps", 0)])
                QTest.keyClick(canvas, Qt.Key.Key_Escape)
                QTest.mouseRelease(canvas, Qt.MouseButton.LeftButton, pos=self.point(*end))
                self.assertEqual(window.selection_refs(), walls)
                self.assertIsNone(window.pending_block)
                self.assertIsNone(canvas.input.gesture)
                self.assertEqual(window.map_data, before)

    def test_a_tile_move_hides_only_the_nested_maps_it_carries(self):
        window = self.window
        data = empty_map(12, 12)
        data["checkpoints"] = []
        child = empty_map(1, 1)
        child["checkpoints"] = []
        data["nested_geometry"] = {"tile": child}
        data["nested_maps"] = [nested("tile", 0, [0, 0], [6, 6]), nested("tile", 0, [2, 2], [2, 2])]
        window.doc.replace_with_new(data)
        window.selection_kind_changed("Tiles")
        window.set_tile_selection((1, 1, 5, 5))
        self.assertTrue(window.begin_transfer(point=QPointF(2.5, 2.5)))
        self.assertEqual(window.moving_nested_maps(), {1})
        window.cancel_interaction()
        self.assertTrue(window.begin_transfer(duplicate=True))
        self.assertEqual(window.moving_nested_maps(), set())
        window.cancel_interaction()
        window.selection_kind_changed("Objects")
        window.inspect_refs([ElementRef("nested_maps", 0)])
        self.assertTrue(window.begin_transfer(point=QPointF(0.5, 0.5)))
        self.assertEqual(window.moving_nested_maps(), {0})
