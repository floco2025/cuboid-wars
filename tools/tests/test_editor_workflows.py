import copy
import json
from unittest.mock import patch

from PySide6.QtCore import QPoint, QPointF, Qt
from PySide6.QtGui import QWheelEvent
from PySide6.QtTest import QTest
from PySide6.QtWidgets import QApplication, QComboBox, QPushButton, QSpinBox

from editor_fixtures import DEFAULT_ALIAS, WindowTestCase, floor, furnished_map, nested
from map_editor import constants as c
from map_editor.elements import ElementRef, refs_for_hit
from map_editor.normalization import empty_level, empty_map


class EditorWorkflowTests(WindowTestCase):
    def set_data(self, data):
        self.window.doc.replace_with_new(data)
        self.window.undo_stack.clear()
        self.app.processEvents()

    def drag(self, start, end):
        canvas = self.window.canvas
        a = canvas.viewport.from_grid(QPointF(*start)).toPoint()
        b = canvas.viewport.from_grid(QPointF(*end)).toPoint()
        QTest.mousePress(canvas, Qt.MouseButton.LeftButton, pos=a)
        QTest.mouseMove(canvas, b)
        QTest.mouseRelease(canvas, Qt.MouseButton.LeftButton, pos=b)
        self.app.processEvents()

    def edit_text(self, widget, text, *, finish=False):
        widget.setFocus()
        widget.selectAll()
        QTest.keyClicks(widget, text)
        if finish:
            QTest.keyClick(widget, Qt.Key.Key_Return)

    def test_sampling_preserves_each_material_face_and_a_new_material_clears_the_sample(self):
        data = empty_map(8, 8)
        data["checkpoints"] = []
        source = floor(1, 1)
        source["north"], source["top"] = "wall", "floor-a"
        data["levels"][0]["floors"] = [source]
        self.set_data(data)
        before = copy.deepcopy(self.window.map_data)
        self.window.sample_at(QPointF(1.5, 1.5))
        self.assertEqual(self.window.mode, c.MODE_FLOOR)
        self.assertEqual(self.window.map_data, before)
        self.window.add_floor_rect((3, 3), (3, 3))
        placed = next(e for e in self.window.map_data["levels"][0]["floors"] if e["col"] == 3)
        self.assertEqual((placed["top"], placed["north"], placed["bottom"]), ("floor-a", "wall", DEFAULT_ALIAS))
        self.window.set_placement_material("floor-b")
        self.window.add_floor_rect((4, 4), (4, 4))
        placed = next(e for e in self.window.map_data["levels"][0]["floors"] if e["col"] == 4)
        self.assertTrue(all(placed[face] == "floor-b" for face in c.FACES))

    def test_sampling_actor_zone_reuses_count_list_respawn_roam_and_controls(self):
        data = furnished_map()
        data["actor_spawn_zones"] = [
            {
                "level": 0,
                "levels": 1,
                "cols": [2, 3],
                "rows": [2, 3],
                "kind": "scuttler",
                "count": [2, 4, 6],
                "respawn_secs": None,
                "beam_in_secs": 2.0,
                "roam_distance": 7.5,
                "switch": "barrier_1",
                "initially_on": False,
            }
        ]
        self.set_data(data)
        self.window.sample_at(QPointF(2.5, 2.5))
        self.assertEqual(self.window.mode, c.MODE_ACTOR_SPAWN_ZONE)
        with patch(
            "map_editor.placement.ActorSpawnFieldsDialog.prompt", side_effect=AssertionError("Unexpected dialog")
        ):
            self.window.add_actor_spawn_zone_rect((5, 5), (5, 5))
        placed = self.window.map_data["actor_spawn_zones"][-1]
        self.assertEqual(
            (placed["kind"], placed["count"], placed["respawn_secs"], placed["beam_in_secs"], placed["roam_distance"]),
            ("scuttler", [2, 4, 6], None, 2.0, 7.5),
        )
        self.assertEqual((placed["switch"], placed["initially_on"]), ("barrier_1", False))

    def test_selection_scope_controls_copy_and_delete_without_prompts_and_clamps_after_level_removal(self):
        data = empty_map(8, 8)
        data["checkpoints"] = []
        data["levels"][0]["floors"] = [floor(1, 1)]
        data["levels"].append(empty_level(1))
        data["levels"][1]["floors"] = [floor(1, 1)]
        self.set_data(data)
        self.window.set_tile_selection((1, 1, 2, 2))
        scope = self.window.tool_settings.findChild(QSpinBox)
        scope.setValue(2)
        with patch("PySide6.QtWidgets.QInputDialog.getInt", side_effect=AssertionError("Unexpected dialog")):
            self.window.copy_selection()
            self.window.delete_selection()
        self.assertEqual(len(self.window.tile_clipboard["levels"]), 2)
        self.assertTrue(all(not level["floors"] for level in self.window.map_data["levels"]))
        self.window.undo_stack.undo()
        self.assertTrue(all(len(level["floors"]) == 1 for level in self.window.map_data["levels"]))
        self.window.doc.replace_with_new(empty_map(8, 8))
        self.assertEqual(self.window.selection_levels, 1)

    def test_dragging_selection_moves_it_once_with_undo(self):
        window = self.window
        before = copy.deepcopy(window.map_data)
        window.set_tile_selection((1, 1, 2, 2))
        self.drag((1.5, 1.5), (4.5, 3.5))
        floors = window.map_data["levels"][0]["floors"]
        self.assertEqual([(e["col"], e["row"]) for e in floors], [(4, 3), (7, 7)])
        self.assertEqual(window.undo_stack.count(), 1)
        window.undo_stack.undo()
        self.assertEqual(window.map_data, before)

    def test_duplicate_preview_does_not_edit_until_placed_and_does_not_replace_clipboard(self):
        window = self.window
        window.set_tile_selection((1, 1, 2, 2))
        window.copy_selection()
        clipboard = copy.deepcopy(window.tile_clipboard)
        before = copy.deepcopy(window.map_data)
        window.duplicate_action.trigger()
        self.assertIsNotNone(window.pending_block)
        self.assertEqual(window.map_data, before)
        self.drag((4.5, 4.5), (4.5, 4.5))
        self.assertEqual(len(window.map_data["levels"][0]["floors"]), 3)
        self.assertEqual(window.tile_clipboard, clipboard)
        window.undo_stack.undo()
        self.assertEqual(window.map_data, before)

    def test_rotation_is_previewed_before_placement_and_undo_restores_the_source(self):
        data = empty_map(8, 8)
        data["checkpoints"] = []
        data["levels"][0]["floors"] = [floor(1, 1), floor(2, 1)]
        self.set_data(data)
        window = self.window
        before = copy.deepcopy(window.map_data)
        window.set_tile_selection((1, 1, 3, 2))
        window.rotate_action.trigger()
        self.assertEqual((window.pending_block.block["grid_cols"], window.pending_block.block["grid_rows"]), (1, 2))
        self.assertEqual(window.map_data, before)
        self.drag((4.5, 3.5), (4.5, 3.5))
        self.assertEqual([(e["col"], e["row"]) for e in window.map_data["levels"][0]["floors"]], [(4, 3), (4, 4)])
        window.undo_stack.undo()
        self.assertEqual(window.map_data, before)

    def test_out_of_bounds_duplicate_can_be_cancelled_without_mutation(self):
        window = self.window
        before = copy.deepcopy(window.map_data)
        window.set_tile_selection((1, 1, 2, 2))
        window.duplicate_selection()
        window.move_pending_block(QPointF(8.5, 8.5))
        window.commit_pending_block()
        self.assertIsNotNone(window.pending_block)
        self.assertEqual(window.map_data, before)
        window.clear_selection()
        self.assertIsNone(window.pending_block)

    def test_inspector_edits_mixed_materials_as_one_undo_without_changing_other_faces(self):
        data = empty_map(8, 8)
        data["checkpoints"] = []
        a, b = floor(1, 1), floor(2, 1)
        a["top"] = "floor-a"
        b["top"] = "floor-b"
        data["levels"][0]["floors"] = [a, b]
        self.set_data(data)
        window = self.window
        before = copy.deepcopy(window.map_data)
        window.set_tile_selection((1, 1, 3, 2))
        inspector = window.properties_panel
        top = inspector.widgets[("top",)]
        self.assertEqual(top.currentText(), "Mixed / unchanged")
        top.setCurrentIndex(top.findData("slab"))
        self.assertTrue(
            all(e["top"] == "slab" and e["bottom"] == DEFAULT_ALIAS for e in window.map_data["levels"][0]["floors"])
        )
        self.assertEqual(window.undo_stack.count(), 1)
        window.undo_stack.undo()
        self.assertEqual(window.map_data, before)

    def test_invalid_property_input_does_not_mutate_and_can_be_corrected(self):
        data = empty_map(8, 8)
        data["checkpoints"] = []
        data["actor_spawn_zones"] = [
            {"level": 0, "cols": [1, 2], "rows": [1, 2], "kind": "scuttler", "count": [2, 4], "respawn_secs": None}
        ]
        self.set_data(data)
        window = self.window
        before = copy.deepcopy(window.map_data)
        window.inspect_hit((c.HIT_SPAWN_ZONE, ("actor_spawn_zones", 0)))
        field = window.properties_panel.widgets[("count",)]
        self.edit_text(field, "4, 2", finish=True)
        self.assertEqual(window.map_data, before)
        self.assertFalse(window.properties_panel.error.isHidden())
        self.edit_text(field, "4, 6", finish=True)
        self.assertEqual(window.map_data["actor_spawn_zones"][0]["count"], [4, 6])
        self.assertTrue(window.properties_panel.error.isHidden())

    def two_actor_zones(self):
        data = empty_map(8, 8)
        data["checkpoints"] = []
        data["actor_spawn_zones"] = [
            {"level": 0, "cols": [1, 2], "rows": [1, 2], "kind": "scuttler", "count": [2, 4], "respawn_secs": None},
            {"level": 0, "cols": [4, 5], "rows": [4, 5], "kind": "scuttler", "count": [1], "respawn_secs": None},
        ]
        self.set_data(data)

    def test_leaving_a_field_commits_it_and_keeps_the_form(self):
        self.two_actor_zones()
        window = self.window
        panel = window.properties_panel
        window.inspect_hit((c.HIT_SPAWN_ZONE, ("actor_spawn_zones", 0)))
        field = panel.widgets[("count",)]
        self.edit_text(field, "4,6")
        self.assertEqual(window.map_data["actor_spawn_zones"][0]["count"], [2, 4])
        window.canvas.setFocus()
        self.assertEqual(window.map_data["actor_spawn_zones"][0]["count"], [4, 6])
        self.assertIs(panel.widgets[("count",)], field)
        self.assertEqual(field.text(), "4, 6")
        self.assertEqual(window.selection_refs(), [ElementRef("actor_spawn_zones", 0)])

    def test_a_selection_change_commits_a_focused_draft_and_reports_an_invalid_one(self):
        self.two_actor_zones()
        window = self.window
        panel = window.properties_panel
        window.inspect_hit((c.HIT_SPAWN_ZONE, ("actor_spawn_zones", 0)))
        self.edit_text(panel.widgets[("count",)], "4, 6")
        window.inspect_hit((c.HIT_SPAWN_ZONE, ("actor_spawn_zones", 1)))
        self.assertEqual([zone["count"] for zone in window.map_data["actor_spawn_zones"]], [[4, 6], [1]])
        self.assertEqual(panel.widgets[("count",)].text(), "1")
        self.edit_text(panel.widgets[("count",)], "3, ")
        with patch.object(window, "notify") as notify:
            window.clear_selection()
        self.assertIn("discarded", notify.call_args.args[0])
        self.assertEqual(window.map_data["actor_spawn_zones"][1]["count"], [1])

    def test_property_commits_share_an_undo_step_until_the_selection_changes(self):
        self.two_actor_zones()
        window = self.window
        panel = window.properties_panel
        before = copy.deepcopy(window.map_data)
        window.inspect_hit((c.HIT_SPAWN_ZONE, ("actor_spawn_zones", 0)))
        self.edit_text(panel.widgets[("count",)], "4, 6", finish=True)
        self.edit_text(panel.widgets[("roam_distance",)], "3", finish=True)
        self.assertEqual(window.undo_stack.count(), 1)
        window.inspect_hit((c.HIT_SPAWN_ZONE, ("actor_spawn_zones", 1)))
        window.inspect_hit((c.HIT_SPAWN_ZONE, ("actor_spawn_zones", 0)))
        self.edit_text(panel.widgets[("roam_distance",)], "5", finish=True)
        self.assertEqual(window.undo_stack.count(), 2)
        # Edits that cancel out within a visit leave no step behind.
        self.edit_text(panel.widgets[("roam_distance",)], "3", finish=True)
        self.assertEqual(window.undo_stack.count(), 1)
        window.undo_stack.undo()
        self.assertEqual(window.map_data, before)

    def test_the_wheel_over_a_property_dropdown_leaves_its_value(self):
        self.two_actor_zones()
        window = self.window
        window.inspect_hit((c.HIT_SPAWN_ZONE, ("actor_spawn_zones", 0)))
        box = window.properties_panel.widgets[("kind",)]
        self.assertGreater(box.count(), 1)
        box.setCurrentIndex(0)
        before = box.currentIndex()
        center = QPointF(box.rect().center())
        event = QWheelEvent(
            center,
            QPointF(box.mapToGlobal(center.toPoint())),
            QPoint(),
            QPoint(0, -120),
            Qt.MouseButton.NoButton,
            Qt.KeyboardModifier.NoModifier,
            Qt.ScrollPhase.NoScrollPhase,
            False,
        )
        QApplication.sendEvent(box, event)
        self.assertEqual(box.currentIndex(), before)
        self.assertEqual(window.undo_stack.count(), 0)

    def test_autosave_save_and_ui_refresh_preserve_a_property_draft(self):
        window = self.window
        data = copy.deepcopy(window.map_data)
        data["actor_spawn_zones"] = [
            {"level": 0, "cols": [1, 2], "rows": [1, 2], "kind": "scuttler", "count": [2, 4], "respawn_secs": None}
        ]
        window.apply_change("Add actor zone", data)
        window.inspect_hit((c.HIT_SPAWN_ZONE, ("actor_spawn_zones", 0)))
        panel = window.properties_panel
        field = panel.widgets[("count",)]
        self.edit_text(field, "4, ")
        field.setSelection(0, 1)
        for refresh in (window._tick_autosave, window.save, window.refresh_ui):
            with self.subTest(refresh=refresh.__name__):
                refresh()
                QTest.qWait(250)
                self.assertIs(panel.widgets[("count",)], field)
                self.assertEqual(field.text(), "4, ")
                self.assertEqual(field.selectedText(), "4")
                self.assertTrue(field.hasFocus())
                self.assertEqual(panel.changed_keys, {("count",)})
                self.assertEqual(window.map_data["actor_spawn_zones"][0]["count"], [2, 4])
        self.edit_text(field, "4, 6", finish=True)
        self.assertEqual(window.map_data["actor_spawn_zones"][0]["count"], [4, 6])
        window.undo_stack.undo()
        window.inspect_hit((c.HIT_SPAWN_ZONE, ("actor_spawn_zones", 0)))
        self.assertEqual(panel.widgets[("count",)].text(), "2, 4")
        window.undo_stack.redo()
        window.inspect_hit((c.HIT_SPAWN_ZONE, ("actor_spawn_zones", 0)))
        self.assertEqual(panel.widgets[("count",)].text(), "4, 6")

    def test_catalog_reload_preserves_invalid_drafts_until_escape_discards_them(self):
        window = self.window
        data = copy.deepcopy(window.map_data)
        data["actor_spawn_zones"] = [
            {"level": 0, "cols": [1, 2], "rows": [1, 2], "kind": "scuttler", "count": [2, 4], "respawn_secs": None}
        ]
        window.apply_change("Add actor zone", data)
        window.inspect_hit((c.HIT_SPAWN_ZONE, ("actor_spawn_zones", 0)))
        panel = window.properties_panel
        count = panel.widgets[("count",)]
        self.edit_text(count, "4, ")
        actor = panel.widgets[("kind",)]
        self.edit_text(actor.lineEdit(), "unfinished")
        gameplay = json.loads(self.global_path.read_text())
        gameplay["actors"]["crawler"] = {"immovable": False}
        self.global_path.write_text(json.dumps(gameplay))
        window.reload_dependencies()
        self.assertIn("crawler", window.actor_kinds)
        self.assertIs(panel.widgets[("count",)], count)
        self.assertIs(panel.widgets[("kind",)], actor)
        self.assertEqual(count.text(), "4, ")
        self.assertEqual(actor.currentText(), "unfinished")
        panel.commit()
        self.assertFalse(panel.error.isHidden())
        self.assertEqual(count.text(), "4, ")
        QTest.keyClick(count, Qt.Key.Key_Escape)
        self.assertEqual(panel.widgets[("count",)].text(), "2, 4")
        self.assertEqual(panel.widgets[("kind",)].currentText(), "scuttler")
        self.assertGreaterEqual(panel.widgets[("kind",)].findData("crawler"), 0)
        self.assertFalse(panel.changed_keys)

    def test_plate_links_include_other_levels_and_nested_geometry(self):
        data = empty_map(8, 8)
        data["checkpoints"] = []
        data["levels"].append(empty_level(1))
        data["switches"] = [{"id": "door", "activation": "momentary", "reset": "never", "hold": "any"}]
        data["levels"][0]["floors"] = [floor(1, 1)]
        data["pressure_plates"] = [{"level": 0, "col": 1, "row": 1, "switch": "door"}]
        data["fields"] = [{"id": "treasure", "color": "#ff3333", "switch": "door"}, {"id": "vault", "color": "#f0c020"}]
        data["levels"][1]["barriers"] = [
            {"c0": 2, "r0": 2, "c1": 3, "r1": 2, "field": "treasure"},
            {"c0": 4, "r0": 2, "c1": 5, "r1": 2, "field": "vault"},
        ]
        child = empty_map(2, 2)
        child["checkpoints"] = []
        child["levels"][0]["light_bridges"] = [{"col": 1, "row": 1, "field": "treasure"}]
        child["actor_spawn_zones"] = [
            {
                "level": 0,
                "cols": [0, 1],
                "rows": [0, 1],
                "kind": "scuttler",
                "count": [2],
                "respawn_secs": None,
                "switch": "door",
            }
        ]
        data["nested_geometry"] = {"room": child}
        data["nested_maps"] = [nested("room", 0, [5, 5], [5, 5])]
        self.set_data(data)
        window = self.window
        window.inspect_hit((c.HIT_PRESSURE_PLATE, (1, 1)))
        links = window.connection_overlay
        self.assertEqual(
            sorted((link.map_name or "", link.ref.name) for link in links.connections),
            [("", "barriers"), ("", "pressure_plates"), ("room", "actor_spawn_zones"), ("room", "light_bridges")],
        )
        self.assertTrue(all(link.switch == "door" for link in links.connections))
        window.set_level_index(1)
        window.inspect_hit((c.HIT_BARRIER, (2, 2, 3, 2)))
        self.assertEqual(len(links.connections), 4)
        window.inspect_hit((c.HIT_BARRIER, (4, 2, 5, 2)))
        self.assertEqual(links.connections, [])

    def test_rotating_nested_geometry_creates_a_copy_and_undo_restores_the_whole_document(self):
        data = empty_map(8, 8)
        data["checkpoints"] = []
        child = empty_map(2, 1)
        child["checkpoints"] = []
        child["levels"][0]["floors"] = [floor(0, 0)]
        data["nested_geometry"] = {"room": child}
        data["nested_maps"] = [nested("room", 0, [1, 1], [1, 1])]
        self.set_data(data)
        window = self.window
        before = copy.deepcopy(window.doc.root_data)
        window.set_tile_selection((1, 1, 3, 2))
        window.rotate_action.trigger()
        window.rotate_action.trigger()
        window.move_pending_block(QPointF(4.5, 4.5))
        window.commit_pending_block()
        self.assertIsNone(window.pending_block)
        entry = window.map_data["nested_maps"][0]
        self.assertNotEqual(entry["map"], "room")
        self.assertEqual(entry["from"], [4, 4])
        self.assertEqual(set(window.doc.nested_geometry), {"room", entry["map"]})
        self.assertEqual(window.doc.nested_geometry["room"], before["nested_geometry"]["room"])
        transformed = window.doc.nested_geometry[entry["map"]]
        self.assertEqual([(e["col"], e["row"]) for e in transformed["levels"][0]["floors"]], [(1, 0)])
        self.assertEqual(window.undo_stack.count(), 1)
        window.undo_stack.undo()
        self.assertEqual(window.doc.root_data, before)
        window.undo_stack.redo()
        self.assertEqual(window.map_data["nested_maps"][0], entry)

    def test_malformed_authored_count_can_be_selected_and_corrected_in_the_inspector(self):
        data = empty_map(8, 8)
        data["checkpoints"] = []
        data["actor_spawn_zones"] = [
            {"level": 0, "cols": [1, 2], "rows": [1, 2], "kind": "scuttler", "count": "invalid", "respawn_secs": None}
        ]
        self.set_data(data)
        self.window.inspect_hit((c.HIT_SPAWN_ZONE, ("actor_spawn_zones", 0)))
        inspector = self.window.properties_panel
        field = inspector.widgets[("count",)]
        self.assertEqual(field.text(), "invalid")
        self.edit_text(field, "2, 3", finish=True)
        self.assertEqual(self.window.map_data["actor_spawn_zones"][0]["count"], [2, 3])

    def test_reversed_wall_endpoints_are_picked_like_any_wall(self):
        data = empty_map(8, 8)
        data["checkpoints"] = []
        data["levels"][0]["floors"] = [floor(1, 1)]
        data["levels"][0]["walls"] = [{"c0": 2, "r0": 1, "c1": 1, "r1": 1, "all": DEFAULT_ALIAS}]
        self.set_data(data)
        window = self.window
        hit = window.hit_at(QPointF(1.5, 1.0))
        self.assertEqual(hit, (c.HIT_WALL, (1, 1, 2, 1)))
        self.assertEqual(refs_for_hit(window.map_data, 0, hit), [ElementRef("walls", 0, 0)])
        window.sample_at(QPointF(1.5, 1.0))
        self.assertEqual(window.mode, c.MODE_WALL)
        window.erase_hit(hit)
        self.assertEqual(window.map_data["levels"][0]["walls"], [])

    def test_a_barrier_or_bridge_edits_and_samples_only_its_field(self):
        data = empty_map(8, 8)
        data["checkpoints"] = []
        data["switches"] = [{"id": "door", "activation": "toggle", "reset_on_player_death": "never"}]
        data["fields"] = [{"id": "gate", "color": "#ff0000", "switch": "door"}, {"id": "walk", "color": "#00ff00"}]
        data["levels"][0]["barriers"] = [{"c0": 1, "r0": 1, "c1": 2, "r1": 1, "field": "gate"}]
        data["levels"][0]["light_bridges"] = [{"col": 4, "row": 4, "field": "walk"}]
        self.set_data(data)
        window = self.window
        for point, mode, attribute, field in (
            (QPointF(1.5, 1.0), c.MODE_BARRIER, "recent_barrier_field", "gate"),
            (QPointF(4.5, 4.5), c.MODE_LIGHT_BRIDGE, "recent_bridge_field", "walk"),
        ):
            with self.subTest(mode=mode):
                setattr(window, attribute, None)
                window.sample_at(point)
                self.assertEqual((window.mode, getattr(window, attribute)), (mode, field))
                combos = window.tool_settings.body.findChildren(QComboBox)
                self.assertEqual([combo.accessibleName() for combo in combos], ["Field"])
                self.assertEqual(window.tool_settings.body.findChildren(QPushButton), [])
                window.inspect_hit(window.hit_at(point))
                self.assertEqual(list(window.properties_panel.widgets), [("field",)])

    def test_sampling_a_plate_without_a_switch_keeps_the_current_switch(self):
        data = furnished_map()
        data["items"] = []
        data["pressure_plates"] = [{"level": 0, "col": 2, "row": 2}]
        self.set_data(data)
        window = self.window
        window.recent_pressure_plate_switch = "barrier_1"
        window.sample_at(QPointF(2.5, 2.5))
        self.assertEqual(window.mode, c.MODE_PRESSURE_PLATE)
        self.assertEqual(window.recent_pressure_plate_switch, "barrier_1")

    def test_inspector_keeps_the_selection_through_normalization_and_no_op_edits(self):
        data = empty_map(8, 8)
        data["checkpoints"] = []
        data["levels"].append(empty_level(1))
        data["actor_spawn_zones"] = [
            {
                "level": 0,
                "levels": 2,
                "cols": [1, 2],
                "rows": [1, 2],
                "kind": "scuttler",
                "count": [2],
                "respawn_secs": None,
            }
        ]
        self.set_data(data)
        window = self.window
        window.inspect_hit((c.HIT_SPAWN_ZONE, ("actor_spawn_zones", 0)))
        inspector = window.properties_panel
        self.edit_text(inspector.widgets[("levels",)], "1", finish=True)
        self.assertNotIn("levels", window.map_data["actor_spawn_zones"][0])
        self.assertEqual(window.selection_refs(), [ElementRef("actor_spawn_zones", 0)])
        self.edit_text(inspector.widgets[("levels",)], "1", finish=True)
        self.assertEqual(window.selection_refs(), [ElementRef("actor_spawn_zones", 0)])
        self.assertEqual(window.undo_stack.count(), 1)

    def test_a_click_inside_the_selection_inspects_and_only_a_drag_lifts_the_block(self):
        data = empty_map(8, 8)
        data["checkpoints"] = []
        data["levels"][0]["floors"] = [floor(1, 1)]
        data["actor_spawn_zones"] = [
            {"level": 0, "cols": [2, 6], "rows": [2, 6], "kind": "scuttler", "count": [1], "respawn_secs": None}
        ]
        self.set_data(data)
        window = self.window
        before = copy.deepcopy(window.map_data)
        window.selection_kind_changed("Tiles")
        window.set_tile_selection((0, 0, 4, 4))
        with patch.object(window, "notify") as notify:
            self.click(1, 1)
        notify.assert_not_called()
        self.assertEqual(window.selection.area.rect, (0, 0, 4, 4))
        self.assertIsNone(window.pending_block)
        self.assertEqual(window.selection_refs(), [ElementRef("floors", 0, 0), ElementRef("actor_spawn_zones", 0)])
        with patch.object(window, "notify") as notify:
            self.drag((1.5, 1.5), (3.5, 3.5))
        self.assertIn("crosses a spawn zone", notify.call_args.args[0])
        self.assertEqual(window.selection.area.rect, (0, 0, 4, 4))
        self.assertEqual(window.map_data, before)

    def test_duplicate_keeps_a_pending_transform_and_cancelling_it_notifies(self):
        data = empty_map(8, 8)
        data["checkpoints"] = []
        data["levels"][0]["floors"] = [floor(1, 1), floor(2, 1)]
        self.set_data(data)
        window = self.window
        window.set_tile_selection((1, 1, 3, 2))
        window.rotate_action.trigger()
        with patch.object(window, "notify") as notify:
            window.duplicate_selection()
        notify.assert_called_once()
        self.assertEqual((window.pending_block.block["grid_cols"], window.pending_block.block["grid_rows"]), (1, 2))
        self.assertFalse(window.pending_block.duplicate)
        with patch.object(window, "notify") as notify:
            window.clear_selection()
        notify.assert_called_once_with("Pending selection cancelled")
        self.assertIsNone(window.pending_block)

    def test_multilevel_paste_onto_the_top_storey_appends_levels(self):
        data = empty_map(8, 8)
        data["checkpoints"] = []
        data["levels"].append(empty_level(1))
        data["levels"][0]["floors"] = [floor(1, 1)]
        data["levels"][1]["floors"] = [floor(1, 1)]
        self.set_data(data)
        window = self.window
        window.set_tile_selection((1, 1, 2, 2))
        window.tool_settings.findChild(QSpinBox).setValue(2)
        window.copy_selection()
        window.set_level_index(1)
        window.set_tile_selection((4, 4, 5, 5))
        window.paste_selection()
        self.assertEqual(len(window.map_data["levels"]), 3)
        self.assertEqual([(e["col"], e["row"]) for e in window.map_data["levels"][2]["floors"]], [(4, 4)])

    def test_the_fireworks_target_is_listed_with_its_plates(self):
        data = furnished_map()
        data["items"] = []
        data["fireworks"] = {"switch": "fireworks"}
        data["pressure_plates"] = [{"level": 0, "col": 1, "row": 1, "switch": "fireworks"}]
        self.set_data(data)
        window = self.window
        window.inspect_hit((c.HIT_PRESSURE_PLATE, (1, 1)))
        connections = window.connection_overlay.connections
        self.assertEqual(len(connections), 2)
        self.assertTrue(any(link.role == "Fireworks" and link.ref is None for link in connections))
