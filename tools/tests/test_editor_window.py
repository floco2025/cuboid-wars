import copy
import json
from pathlib import Path
from unittest.mock import patch

from PySide6.QtCore import QEvent, QPoint, QPointF, Qt
from PySide6.QtGui import QContextMenuEvent, QMouseEvent
from PySide6.QtTest import QSignalSpy, QTest
from PySide6.QtWidgets import QComboBox, QDialog, QMenu, QMessageBox

from editor_fixtures import DEFAULT_ALIAS, WindowTestCase
from map_editor.constants import (
    FACES,
    HIT_ITEM,
    HIT_LADDER,
    HIT_TERRAIN,
    MODE_ACTOR_SPAWN_ZONE,
    MODE_ERASE,
    MODE_ERASE_LADDERS,
    MODE_FLOOR,
    MODE_FLOOR_MATERIAL,
    MODE_ITEM,
    MODE_LADDER,
    MODE_LIGHT,
    MODE_NESTED_MAP,
    MODE_PRESSURE_PLATE,
    MODE_RAMP,
    TERRAIN_FACES,
    MODE_SELECT,
    MODE_WALL,
)
from map_editor.dependencies import MapDependencies
from map_editor.dialogs import ActorSpawnFieldsDialog
from map_editor.editing import paint_edges, paint_floors
from map_editor.io import write_map
from map_editor.nesting import NestedMotion
from map_editor.normalization import empty_map
from map_editor.transforms import insert_level_data
from map_editor.window import EditorWindow


class WindowTests(WindowTestCase):
    def test_paste_beside_an_invalid_item_is_not_refused_for_its_shifted_index(self):
        window = self.window
        data = copy.deepcopy(window.map_data)
        data["levels"][0]["floors"].append({"col": 6, "row": 6, "all": DEFAULT_ALIAS})
        data["items"] = [{"level": 0, "col": 6, "row": 6, "type": "not_a_type"}]
        window.doc.replace_with_new(data)
        self.click(1, 1)
        window.copy_action.trigger()
        self.click(3, 3)
        with patch.object(window, "notify") as refused:
            window.paste_action.trigger()
        refused.assert_not_called()
        self.assertEqual(len(window.map_data["levels"][0]["floors"]), 4)

    def test_close_saves_geometry_shared_with_other_maps_but_cancel_keeps_window_open(self):
        window = self.window
        window.resize(600, 450)
        window.move(60, 70)
        self.app.processEvents()
        normal = window.geometry()
        with (
            patch.object(window, "confirm_discard_changes", return_value=False),
            patch.object(window.window_geometry, "save") as save,
        ):
            self.assertFalse(window.close())
            self.assertTrue(window.isVisible())
            save.assert_not_called()
        with patch.object(window, "confirm_discard_changes", return_value=True):
            self.assertTrue(window.close())
        self.assertFalse(window.window_geometry.timer.isActive())
        self.assertEqual(window.preferences.value(window.window_geometry.KEY), window.saveGeometry())
        other_path = self.path.parent.parent / "obby" / "layout.json"
        write_map(other_path, empty_map(12, 16))
        self.window = EditorWindow(other_path, preferences=window.preferences)
        window.deleteLater()
        self.window._autosave_timer.stop()
        self.window.show()
        self.app.processEvents()
        self.assertEqual(self.window.geometry(), normal)

    def test_map_operations_fit_canvas_without_changing_window_geometry(self):
        window = self.window
        window.resize(600, 450)
        window.move(60, 70)
        for maximized in (False, True):
            with self.subTest(maximized=maximized):
                if maximized:
                    window.showMaximized()
                self.app.processEvents()
                geometry = window.geometry()
                normal = window.normalGeometry()
                with (
                    patch("map_editor.structure.ResizeMapDialog.prompt", return_value=(12, 12, 0, 0)),
                    patch.object(window, "confirm_discard_changes", return_value=True),
                    patch("map_editor.file_actions.QMessageBox.warning"),
                    patch("map_editor.file_actions.QMessageBox.question", return_value=QMessageBox.StandardButton.Yes),
                    patch.object(window, "choose_map_path", return_value=self.path),
                ):
                    for action in (window.resize_map, window.new_file, lambda: window.load_path(self.path)):
                        window.canvas.zoom_by(2)
                        action()
                        self.app.processEvents()
                        self.assertTrue(window.canvas.viewport.fitted)
                        self.assertEqual(window.geometry(), geometry)
                        self.assertEqual(window.normalGeometry(), normal)
                        self.assertEqual(window.isMaximized(), maximized)

    def move_with_button(self, canvas, position):
        self.app.sendEvent(
            canvas,
            QMouseEvent(
                QEvent.Type.MouseMove,
                QPointF(position),
                QPointF(canvas.mapToGlobal(position)),
                Qt.MouseButton.NoButton,
                Qt.MouseButton.LeftButton,
                Qt.KeyboardModifier.NoModifier,
            ),
        )

    def test_single_tile_tools_preview_and_commit_one_target_without_drag_ranges(self):
        window = self.window
        canvas = window.canvas
        methods = {
            MODE_LADDER: "add_ladder_at",
            MODE_LIGHT: "add_light_at",
            MODE_PRESSURE_PLATE: "prompt_and_add_pressure_plate",
            MODE_ITEM: "prompt_and_add_item",
        }
        for mode, method in methods.items():
            with self.subTest(mode=mode), patch.object(window, method) as place:
                window.set_mode(mode)
                self.app.processEvents()
                start, end = self.point(1.5, 1.1), self.point(4.5, 4.1)
                QTest.mousePress(canvas, Qt.MouseButton.LeftButton, pos=start)
                self.move_with_button(canvas, end)
                self.assertEqual(canvas.hover_cell, (4, 4))
                if mode in (MODE_LADDER, MODE_LIGHT):
                    self.assertEqual(canvas.hover_edge_side, "N")
                place.assert_not_called()
                QTest.mouseRelease(canvas, Qt.MouseButton.LeftButton, pos=end)
                if mode in (MODE_LADDER, MODE_LIGHT):
                    place.assert_called_once_with(canvas.grid_position(end))
                else:
                    place.assert_called_once_with(4, 4)
                self.assertIsNone(canvas.input.gesture)

    def test_single_tile_placement_cancels_on_escape_tool_change_or_off_grid_release(self):
        window = self.window
        canvas = window.canvas
        for cancel in ("escape", "tool", "outside"):
            with self.subTest(cancel=cancel), patch.object(window, "prompt_and_add_pressure_plate") as place:
                window.set_mode(MODE_PRESSURE_PLATE)
                self.app.processEvents()
                position = self.point(2.5, 2.5)
                QTest.mousePress(canvas, Qt.MouseButton.LeftButton, pos=position)
                if cancel == "escape":
                    QTest.keyClick(canvas, Qt.Key.Key_Escape)
                elif cancel == "tool":
                    window.set_mode(MODE_SELECT)
                else:
                    position = QPoint(-20, -20)
                QTest.mouseRelease(canvas, Qt.MouseButton.LeftButton, pos=position)
                place.assert_not_called()
                self.assertIsNone(canvas.input.gesture)
        self.assertTrue(window.selection.empty)

    def test_range_tools_still_receive_both_drag_endpoints(self):
        window = self.window
        canvas = window.canvas
        methods = {
            MODE_FLOOR: "add_floor_rect",
            MODE_ERASE: "erase_cell_rect",
            MODE_FLOOR_MATERIAL: "assign_floor_materials_rect",
            MODE_WALL: "add_wall_line",
            MODE_RAMP: "add_ramp",
            MODE_NESTED_MAP: "add_nested_map",
        }
        for mode, method in methods.items():
            with self.subTest(mode=mode), patch.object(window, method) as place:
                window.set_mode(mode)
                self.app.processEvents()
                start, end = self.point(1.1, 1.1), self.point(4.1, 1.1)
                QTest.mousePress(canvas, Qt.MouseButton.LeftButton, pos=start)
                self.move_with_button(canvas, end)
                self.assertEqual(canvas.drag_start_cell, (1, 1))
                self.assertEqual(canvas.drag_current_cell, (4, 1))
                QTest.mouseRelease(canvas, Qt.MouseButton.LeftButton, pos=end)
                place.assert_called_once()
                self.assertEqual(place.call_args.args[:2], ((1, 1), (4, 1)))

    def test_canvas_notice_replaces_and_expires_without_resizing_or_taking_focus(self):
        window = self.window
        canvas = window.canvas
        canvas.setFocus()
        geometry = canvas.geometry()
        window.notify("No wall here")
        notice = canvas.notice
        self.assertTrue(notice.isVisible())
        self.assertTrue(notice.testAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents))
        self.assertTrue(canvas.hasFocus())
        window.notify("Nothing to erase")
        self.assertEqual(notice.text(), "Nothing to erase")
        self.assertEqual(canvas.geometry(), geometry)
        notice.timer.start(10)
        QTest.qWait(30)
        self.assertFalse(notice.isVisible())
        self.assertEqual(canvas.geometry(), geometry)

    def test_issues_popup_is_available_on_request_and_updates_after_edit_and_undo(self):
        window = self.window
        panel = window.issues_dialog
        self.assertEqual(panel.list.count(), 0)
        data = paint_floors(window.map_data, 0, (6, 5, 7, 6), "not_an_alias")
        window.apply_change("Invalid material", data)
        self.assertGreater(panel.list.count(), 0)
        self.assertIn(str(panel.list.count()), panel.windowTitle())
        self.assertFalse(panel.isVisible())
        window.show_map_issues()
        self.assertTrue(panel.isVisible())
        window.undo_stack.undo()
        self.assertEqual(panel.list.count(), 0)
        self.assertEqual(panel.summary.text(), "No issues")

    def test_middle_and_space_drag_pan_without_erasing(self):
        window = self.window
        window.set_mode(MODE_ERASE)
        canvas = window.canvas
        canvas.zoom_by(3)
        canvas.setFocus()
        before = copy.deepcopy(window.map_data)
        start, end = QPoint(100, 100), QPoint(160, 150)
        for button, space in ((Qt.MouseButton.MiddleButton, False), (Qt.MouseButton.LeftButton, True)):
            origin = QPointF(canvas.viewport.offset)
            if space:
                QTest.keyPress(canvas, Qt.Key.Key_Space)
            QTest.mousePress(canvas, button, pos=start)
            QTest.mouseMove(canvas, end)
            QTest.mouseRelease(canvas, button, pos=end)
            if space:
                QTest.keyRelease(canvas, Qt.Key.Key_Space)
            self.assertEqual(canvas.viewport.offset, origin + QPointF(60, 50))
        self.assertEqual(window.map_data, before)
        self.assertFalse(window.dirty)

    def test_visible_entries_cull_offscreen_geometry(self):
        canvas = self.window.canvas
        canvas.viewport.cell = 100
        canvas.viewport.offset = QPointF(-1000, -1000)
        entries = [{"col": 1, "row": 1}, {"col": 12, "row": 12}]
        self.assertEqual(list(canvas.visible_entries("floors", entries)), entries[1:])

    def test_conflicting_loaded_plates_can_be_erased_independently(self):
        window = self.window
        self.set_switches("a", "b")
        data = copy.deepcopy(window.map_data)
        data["pressure_plates"] = [{"level": 0, "col": 1, "row": 1, "switch": switch} for switch in ("a", "b")]
        window.doc.replace_with_new(data)
        canvas = window.canvas
        position = self.point(1.5, 1.5)

        def choose_erase(menu, *_):
            submenu = next(a.menu() for a in menu.actions() if a.text() == "Select plate")
            action = next(a for a in submenu.actions() if a.text() == "Pressure Plate (a)")
            action.trigger()
            window.delete_action.trigger()

        menu = QMenu(canvas)
        menu.exec = lambda *_: choose_erase(menu)
        with patch("map_editor.interaction.QMenu", return_value=menu):
            canvas.contextMenuEvent(
                QContextMenuEvent(QContextMenuEvent.Reason.Mouse, position, canvas.mapToGlobal(position))
            )
        self.assertEqual([p["switch"] for p in window.plates_at(1, 1)], ["b"])

    def test_invalid_ladders_remain_erasable_without_breaking_other_tools(self):
        window = self.window
        data = copy.deepcopy(window.map_data)
        data["ladders"] = [{"lower_level": 0, "col": 2, "row": 2, "levels": 0, "side": "bad"}]
        window.doc.replace_with_new(data)
        hit = window.hit_at(QPointF(2.5, 2.5))
        self.assertEqual(hit[0], HIT_LADDER)
        window.erase_group_rect(MODE_ERASE_LADDERS, (5, 5), (5, 5))
        self.assertEqual(len(window.map_data["ladders"]), 1)
        window.erase_group_rect(MODE_ERASE_LADDERS, (2, 2), (2, 2))
        self.assertEqual(window.map_data["ladders"], [])
        window.undo_stack.undo()
        self.assertEqual(len(window.map_data["ladders"]), 1)

    def test_terrain_material_editor_exposes_only_sides_and_bottom(self):
        window = self.window
        data = copy.deepcopy(window.map_data)
        level = data["levels"][0]
        level["floors"] = []
        level["terrain"] = [{"col": 1, "row": 1, **dict.fromkeys(TERRAIN_FACES, DEFAULT_ALIAS)}]
        window.apply_change("Terrain", data)
        replacement = window.materials_catalog[1]
        window.inspect_hit((HIT_TERRAIN, (1, 1)), show=True)
        self.assertEqual(set(window.properties_panel.widgets), {(face,) for face in TERRAIN_FACES})
        self.set_property("bottom", replacement)
        terrain = window.map_data["levels"][0]["terrain"][0]
        self.assertEqual(terrain["bottom"], replacement)
        self.assertNotIn("top", terrain)

    def test_material_helpers_fill_every_face_and_share_one_undo_step(self):
        window = self.window
        pattern = dict(zip(FACES, window.materials_catalog[:6]))
        data = copy.deepcopy(window.map_data)
        data["levels"][0]["floors"] = [
            {"col": 1, "row": 1, **pattern},
            {"col": 2, "row": 1, "all": DEFAULT_ALIAS},
        ]
        window.doc.replace_with_new(data)
        before = copy.deepcopy(window.map_data)
        window.assign_floor_materials_rect((1, 1), (2, 1))
        panel = window.properties_panel
        panel.source_button.click()
        self.assertEqual({face: panel.widgets[(face,)].currentData() for face in FACES}, pattern)
        floors = window.map_data["levels"][0]["floors"]
        self.assertTrue(all(floor.get(face, floor.get("all")) == pattern[face] for floor in floors for face in FACES))
        panel.apply_all_button.click()
        self.assertTrue(all(panel.widgets[(face,)].currentData() == pattern["top"] for face in FACES))
        floors = window.map_data["levels"][0]["floors"]
        self.assertTrue(all(floor.get(face, floor.get("all")) == pattern["top"] for floor in floors for face in FACES))
        self.assertEqual(window.undo_stack.count(), 1)
        window.undo_stack.undo()
        self.assertEqual(window.map_data, before)

    def test_top_left_material_pattern_applies_to_selected_floors_and_walls_and_undoes(self):
        window = self.window
        pattern = dict(zip(FACES, window.materials_catalog[:6]))

        for walls in (False, True):
            with self.subTest(walls=walls):
                data = empty_map(8, 8)
                if walls:
                    data["levels"][0]["walls"] = [
                        {"c0": 3, "r0": 3, "c1": 4, "r1": 3, "all": DEFAULT_ALIAS},
                        {"c0": 2, "r0": 2, "c1": 1, "r1": 2, **pattern},
                    ]
                else:
                    data["levels"][0]["floors"] = [{"col": 3, "row": 3, "all": DEFAULT_ALIAS}]
                    data["levels"][0]["inaccessible_floors"] = [{"col": 2, "row": 2, **pattern}]
                window.doc.replace_with_new(data)
                before = copy.deepcopy(window.map_data)
                if walls:
                    window.assign_wall_materials_rect((0, 0), (5, 5))
                else:
                    window.assign_floor_materials_rect((0, 0), (5, 5))
                window.properties_panel.source_button.click()
                level = window.map_data["levels"][0]
                entries = level["walls"] if walls else level["floors"] + level["inaccessible_floors"]
                self.assertTrue(all({face: entry[face] for face in FACES} == pattern for entry in entries))
                window.undo_stack.undo()
                self.assertEqual(window.map_data, before)

    def test_ladders_place_with_the_previous_span_without_a_dialog(self):
        window = self.window
        data = empty_map(8, 8)
        data["levels"] *= 4
        window.doc.replace_with_new(data)
        window.recent_ladder_levels = 2
        with patch.object(QDialog, "exec", side_effect=AssertionError("Unexpected placement dialog")):
            for col in (3, 5):
                window.add_ladder_at(QPointF(col + 0.5, 3.05))
        self.assertEqual([ladder["levels"] for ladder in window.map_data["ladders"]], [2, 2])

    def test_item_and_field_placement_uses_previous_values_without_dialogs(self):
        window = self.window
        window.add_floor_rect((2, 1), (2, 1))
        window.field_colors = {"gate": "#ff0000", "bridge": "#00ff00"}
        window.doc.root_data["fields"] = [{"id": field, "color": color} for field, color in window.field_colors.items()]
        self.set_switches("gate", "bridge")
        window.recent_barrier_field = window.recent_pressure_plate_switch = "gate"
        window.recent_bridge_field = "bridge"
        window.recent_item_type = "health_potion"
        with (
            patch("map_editor.placement.KindDialog.prompt") as kind,
            patch("map_editor.items.ItemTypeDialog.prompt") as item,
        ):
            window.prompt_and_add_barrier_line((1, 1), (2, 1))
            window.prompt_and_add_pressure_plate(1, 1)
            window.prompt_and_add_light_bridge_rect((4, 4), (4, 4))
            window.recent_pressure_plate_switch = "bridge"
            window.prompt_and_add_pressure_plate(2, 1)
            window.prompt_and_add_item(1, 1)
            kind.assert_not_called()
            item.assert_not_called()
        self.assertEqual(window.map_data["items"][0]["type"], "health_potion")
        self.assertEqual([p["switch"] for p in window.map_data["pressure_plates"]], ["gate", "bridge"])
        level = window.map_data["levels"][0]
        self.assertEqual(level["barriers"], [{"c0": 1, "r0": 1, "c1": 2, "r1": 1, "field": "gate"}])
        self.assertEqual(level["light_bridges"], [{"col": 4, "row": 4, "field": "bridge"}])

    def test_a_key_names_its_field_and_loses_it_with_its_type(self):
        window = self.window
        window.recent_item_type = "key"
        window.recent_item_key_field = "treasure"
        window.prompt_and_add_item(1, 1)
        key = {"level": 0, "col": 1, "row": 1, "type": "key", "field": "treasure"}
        self.assertEqual(window.map_data["items"], [key])
        window.inspect_hit((HIT_ITEM, (1, 1)))
        self.set_property("field", "lobby")
        self.assertEqual(window.map_data["items"], [{**key, "field": "lobby"}])
        self.set_property("type", "gold")
        self.assertEqual(window.map_data["items"], [{"level": 0, "col": 1, "row": 1, "type": "gold"}])

    def test_nested_map_placement_reuses_configured_motion(self):
        window = self.window
        window.doc.root_data["nested_geometry"] = {"tile": empty_map(1, 1)}
        window.recent_nested_map = NestedMotion("tile", 0, 3.0, 1.0, 0.0, (0, 0, 0), (0, 0, 0))
        with patch("map_editor.nested_maps.MotionDialog.prompt_nested") as prompt:
            window.add_nested_map((3, 3), (4, 3))
            window.add_nested_map((3, 5), (4, 5))
            prompt.assert_not_called()
        self.assertEqual([entry["travel_secs"] for entry in window.map_data["nested_maps"]], [3.0, 3.0])

    def test_toolbar_switch_choices_follow_the_catalog(self):
        window = self.window
        self.set_switches("a")
        window.set_mode(MODE_PRESSURE_PLATE)
        window.tool_settings.refresh()
        combo = window.tool_settings.body.findChildren(QComboBox)[0]
        self.assertEqual([combo.itemText(i) for i in range(combo.count())], ["", "a"])
        self.set_switches("a", "b")
        window.tool_settings.refresh()
        combo = window.tool_settings.body.findChildren(QComboBox)[0]
        self.assertEqual([combo.itemText(i) for i in range(combo.count())], ["", "a", "b"])

    def test_actor_picker_rejects_unknown_kinds_and_toolbar_reuses_valid_choices(self):
        window = self.window
        kind = window.actor_kinds[0]
        dialog = ActorSpawnFieldsDialog(window, kind, [3], 90, 2.5, ["guards"], None)
        self.assertGreater(dialog._kind_edit.count(), 0)
        self.assertEqual(dialog.values(), (kind, [3], 90, 2.5, None, True, 0, 1, 0.0, None, None))
        dialog._switch_combo.setCurrentText("guards")
        self.assertEqual(dialog.values(), (kind, [3], 90, 2.5, "guards", True, 0, 1, 0.0, None, None))
        dialog.deleteLater()
        with (
            patch.object(ActorSpawnFieldsDialog, "exec", return_value=QDialog.DialogCode.Accepted),
            patch("map_editor.dialogs.catalogs.QMessageBox.warning") as warning,
        ):
            self.assertIsNone(ActorSpawnFieldsDialog.prompt(window, "not_a_kind", [3], 90, 2.5, ["guards"], None))
            warning.assert_called_once()
        self.set_switches("guards")
        window.recent_actor_spawn_kind = kind
        window.recent_actor_spawn_count = [7]
        window.recent_actor_spawn_switch = "guards"
        window.set_mode(MODE_ACTOR_SPAWN_ZONE)
        window.tool_settings.refresh()
        combos = window.tool_settings.findChildren(QComboBox)
        self.assertEqual(combos[0].currentText(), kind)
        self.assertEqual(window.recent_actor_spawn_switch, "guards")
        with patch.object(ActorSpawnFieldsDialog, "prompt") as prompt:
            window.add_actor_spawn_zone_rect((2, 2), (3, 3))
            prompt.assert_not_called()
        zone = window.map_data["actor_spawn_zones"][0]
        self.assertEqual((zone["count"], zone["switch"]), ([7], "guards"))
        self.assertNotIn("initially_on", zone)
        window.recent_actor_spawn_switch = ""
        window.recent_actor_spawn_initially_on = False
        with patch.object(ActorSpawnFieldsDialog, "prompt") as prompt:
            window.add_actor_spawn_zone_rect((4, 4), (5, 5))
        unswitched = window.map_data["actor_spawn_zones"][1]
        self.assertNotIn("switch", unswitched)
        self.assertIs(unswitched["initially_on"], False)

    def test_canvas_letter_shortcuts_do_not_steal_actor_search_text(self):
        window = self.window
        window.set_mode(MODE_ACTOR_SPAWN_ZONE)
        box = window.tool_settings.findChild(QComboBox)
        edit = box.lineEdit()
        edit.setFocus()
        edit.clear()
        window.canvas.viewport.fitted = False
        QTest.keyClicks(edit, "fmlr")
        self.assertEqual(edit.text(), "fmlr")
        self.assertFalse(window.canvas.viewport.fitted)
        self.assertFalse(window.show_material_overlay)
        self.assertFalse(window.show_adjacent_levels)
        self.assertFalse(window.show_roam_extensions)
        window.canvas.setFocus()
        QTest.keyClick(window.canvas, Qt.Key.Key_R)
        self.assertTrue(window.show_roam_extensions)
        self.assertTrue(window.roam_extensions_action.isChecked())
        window.roam_extensions_action.trigger()
        self.assertFalse(window.show_roam_extensions)
        QTest.keyClick(window.canvas, Qt.Key.Key_F)
        self.assertTrue(window.canvas.viewport.fitted)
        QTest.keyClick(window.canvas, Qt.Key.Key_L)
        self.assertTrue(window.show_adjacent_levels)
        QTest.keyClick(window.canvas, Qt.Key.Key_L)
        self.assertFalse(window.show_adjacent_levels)
        QTest.keyClick(window.canvas, Qt.Key.Key_M, Qt.KeyboardModifier.ShiftModifier)
        self.assertFalse(window.show_adjacent_levels)

    def test_clicking_validation_issue_focuses_its_level_and_highlights_object(self):
        window = self.window
        data = insert_level_data(window.map_data, 1)
        data["levels"][1]["floors"] = [{"col": 6, "row": 5, "all": "unknown_alias"}]
        window.apply_change("Bad material", data)
        window.show_map_issues()
        item = window.issues_dialog.list.item(0)
        issue = item.data(Qt.ItemDataRole.UserRole)
        self.assertEqual((issue.level, issue.rect), (1, (6, 5, 7, 6)))
        window.issues_dialog.list.itemClicked.emit(item)
        self.assertEqual(window.current_level, 1)
        self.assertEqual(window.canvas.issue_rects, [(6, 5, 7, 6)])
        center = window.canvas.viewport.from_grid(QPointF(6.5, 5.5))
        self.assertTrue(window.canvas.rect().contains(center.toPoint()))
        self.assertLessEqual(window.canvas.viewport.offset.x(), 0)
        self.assertLessEqual(window.canvas.viewport.offset.y(), 0)

    def test_file_notifications_reload_parent_catalogs(self):
        with patch.object(self.window, "adopt_catalogs") as adopt:
            self.window.dependencies.changed.emit()
        self.assertGreaterEqual(adopt.call_count, 1)
        self.assertEqual(adopt.call_args.args[0], "hotel")
        watcher = MapDependencies(self.window)
        settings = Path(self.temp.name) / "gameplay.json"
        settings.write_text("{}")
        with patch("map_editor.dependencies.GAMEPLAY_PATH", settings):
            watcher.watch("hotel")
            changed = QSignalSpy(watcher.changed)
            settings.write_text('{"maps": {}}')
            for _ in range(30):
                if changed.count():
                    break
                QTest.qWait(100)
            self.assertGreater(changed.count(), 0)
            watcher.watch("hotel")
            self.assertIn(str(settings.resolve()), watcher.watcher.files())

    def test_catalog_watcher_ignores_siblings_and_keeps_watching_replaced_files(self):
        window = self.window
        settings = self.path.with_name("settings.json")
        changed = QSignalSpy(window.dependencies.changed)
        settings.with_name("layout.autosave.json").write_text("{}")
        settings.write_bytes(settings.read_bytes())
        QTest.qWait(250)
        self.assertEqual(changed.count(), 0)
        values = json.loads(settings.read_text())
        values["geometry"]["grid_cell_size"] = 4.0
        replacement = settings.with_suffix(".tmp")
        replacement.write_text(json.dumps(values))
        replacement.replace(settings)
        for _ in range(30):
            if changed.count():
                break
            QTest.qWait(100)
        self.assertEqual(changed.count(), 1)
        self.assertEqual(window.grid_cell_size, 4.0)
        self.assertIn(str(settings.resolve()), window.dependencies.watcher.files())
        values["geometry"]["grid_cell_size"] = 5.0
        settings.write_text(json.dumps(values))
        for _ in range(30):
            if changed.count() == 2:
                break
            QTest.qWait(100)
        self.assertEqual(changed.count(), 2)
        self.assertEqual(window.grid_cell_size, 5.0)

    def test_plates_take_the_games_size_and_follow_edits_beside_them(self):
        window = self.window
        window.grid_cell_size, window.wall_width_cells = 1.0, 0.2
        data = paint_floors(empty_map(7, 7), 0, (0, 0, 7, 7), DEFAULT_ALIAS)
        data["pressure_plates"] = [{"level": 0, "col": 3, "row": 3, "switch": "a"}]
        window.doc.replace_with_new(data)
        plate = window.map_data["pressure_plates"][0]
        self.assertAlmostEqual(window.plate_side(plate), 1.7, msg="open floor")
        self.assertAlmostEqual(
            window.plate_side({"level": 0, "col": 4, "row": 3}), 0.7, msg="one about to go beside it"
        )
        self.assertAlmostEqual(window.plate_side(plate), 1.7, msg="unplaced, it shrinks nothing")
        window.apply_change("Place Wall", paint_edges(window.map_data, 0, (3, 3), (3, 4), material=DEFAULT_ALIAS))
        self.assertAlmostEqual(window.plate_side(plate), 0.7, msg="a wall on its edge")
        self.assertFalse(window.canvas.grab().isNull())

    def test_large_map_fits_and_paints_an_item_and_invalid_nested_nudges(self):
        window = self.window
        data = paint_floors(empty_map(256, 256), 0, (250, 250, 256, 256), DEFAULT_ALIAS)
        data["items"] = [{"level": 0, "col": 250, "row": 250, "type": "portal_gun"}]
        data["nested_maps"] = [
            {"map": "missing", "level": 0, "from": [1, 1], "to": [2, 2], "from_nudge": [0], "to_nudge": [0]}
        ]
        window.doc.replace_with_new(data)
        window.canvas.fit_map()
        corner = window.canvas.viewport.from_grid(QPointF(256, 256))
        self.assertLessEqual(corner.x(), window.canvas.width())
        self.assertLessEqual(corner.y(), window.canvas.height())
        self.assertFalse(window.canvas.grab().isNull())
