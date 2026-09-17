import copy
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from PySide6.QtCore import QPointF, Qt
from PySide6.QtTest import QTest
from PySide6.QtWidgets import QComboBox, QSpinBox

from editor_fixtures import EditorHost, WindowTestCase, floor, nested
from map_editor.checkpoint_numbers import (
    checkpoint_entries,
    next_checkpoint_number,
    number_checkpoint_copies,
    number_generated_definitions,
    renumber_checkpoints,
)
from map_editor.constants import (
    CHECKPOINT_LIST,
    CHECKPOINT_TYPE_LABELS,
    HIT_CHECKPOINT,
    HIT_SPAWN_ZONE,
    MODE_CHECKPOINT,
    MODE_SELECT,
    MODE_ERASE_CHECKPOINTS,
    MODE_ERASE_SPAWN_ZONES,
)
from map_editor.dialogs import CheckpointsDialog
from map_editor.erasing import erase_cell_rect, erase_group_rect, erase_hit, hit_at
from map_editor.io import read_map, write_map
from map_editor.normalization import canonicalize_map, empty_map, normalize_map, zone_key
from map_editor.regions import TileRegion, copy_region, delete_region, paste_region
from map_editor.transforms import insert_level_data, remove_level_data, resize_map_data, translate_map
from map_editor.elements import ElementRef
from map_editor.types import ZoneRef
from map_editor.validation import validate_map


def checkpoint_map(number=1):
    data = empty_map(8, 8)
    data["player_spawn_zones"] = []
    data["levels"][0]["floors"] = [floor(c, r) for c in range(1, 4) for r in range(1, 4)]
    data["checkpoints"] = [{"level": 0, "cols": [1, 4], "rows": [1, 4], "type": "individual", "number": number}]
    return data


def zone(**fields):
    return {"level": 0, "cols": [1, 2], "rows": [1, 2], "kind": "zapper", "count": [1], "respawn_secs": None, **fields}


class CheckpointTests(unittest.TestCase):
    def test_a_checkpoint_under_a_spawn_zone_is_picked_first(self):
        data = checkpoint_map()
        rect = {"level": 0, "cols": [1, 4], "rows": [1, 4]}
        data["player_spawn_zones"] = [dict(rect)]
        data["actor_spawn_zones"] = [{**rect, "kind": "zapper", "count": [1], "respawn_secs": 90}]

        self.assertEqual(hit_at(data, 0, 2.5, 2.5, 0.1), (HIT_CHECKPOINT, (CHECKPOINT_LIST, 0)))
        self.assertEqual(EditorHost(data, []).spawn_zone_at(QPointF(2.5, 2.5)), ZoneRef(CHECKPOINT_LIST, 0))

        data["checkpoints"] = []
        self.assertEqual(hit_at(data, 0, 2.5, 2.5, 0.1), (HIT_SPAWN_ZONE, ("actor_spawn_zones", 0)))
        self.assertEqual(EditorHost(data, []).spawn_zone_at(QPointF(2.5, 2.5)), ZoneRef("actor_spawn_zones", 0))

    def test_same_rectangle_checkpoints_keep_their_identity(self):
        data = checkpoint_map()
        first = data["checkpoints"][0]
        second = {**first, "type": "group_all", "number": 2}
        data["checkpoints"] = [first, second]

        self.assertNotEqual(zone_key(CHECKPOINT_LIST, first), zone_key(CHECKPOINT_LIST, second))
        canonical = canonicalize_map(data)
        self.assertEqual(len(canonical["checkpoints"]), 2)
        host = EditorHost(canonical, [])
        for zone in (first, second):
            ref = host._zone_ref_after_change(CHECKPOINT_LIST, zone)
            self.assertEqual(canonical["checkpoints"][ref.index]["type"], zone["type"])

    def test_roundtrip_including_nested_geometry_and_absent_list(self):
        data = checkpoint_map()
        data["nested_geometry"] = {"platform": checkpoint_map(2)}
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "layout.json"
            write_map(path, data)
            self.assertEqual(read_map(path), normalize_map(data))
        del data["checkpoints"]
        self.assertEqual(normalize_map(data)["checkpoints"], [])

    def test_floor_and_overlap_validation(self):
        data = checkpoint_map()
        self.assertFalse(validate_map(data, [], []))
        for mutate in (
            lambda d: d["levels"][0]["floors"].pop(),
            lambda d: d["checkpoints"].append({**copy.deepcopy(d["checkpoints"][0]), "number": 2}),
            lambda d: d["checkpoints"][0].update(level=2),
            lambda d: d["ramps"].append({"low": [1, 1], "high": [2, 3], "lower_level": 0}),
        ):
            bad = copy.deepcopy(data)
            mutate(bad)
            self.assertTrue(any("checkpoints" in error for error in validate_map(bad, [], [])))

    def test_numbers_are_kept_formatted_and_validated(self):
        data = checkpoint_map(7)
        self.assertEqual(normalize_map(data)["checkpoints"][0]["number"], 7)
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "layout.json"
            write_map(path, data)
            self.assertIn('"number": 7', path.read_text())
            self.assertEqual(read_map(path)["checkpoints"][0]["number"], 7)
        self.assertFalse(validate_map(data, [], []))
        for number in (0, -1, 1.5, "3", True, None):
            bad = copy.deepcopy(data)
            if number is None:
                del bad["checkpoints"][0]["number"]
            else:
                bad["checkpoints"][0]["number"] = number
            self.assertTrue(any("positive whole" in error for error in validate_map(bad, [], [])), number)
        data["checkpoints"][0]["cols"] = [1, 2]
        data["checkpoints"].append({"level": 0, "cols": [2, 4], "rows": [1, 4], "type": "individual", "number": 7})
        self.assertTrue(any("already used" in error for error in validate_map(data, [], [])))

    def test_saved_checkpoints_sort_by_number(self):
        data = checkpoint_map(5)
        data["checkpoints"][0]["cols"] = [1, 2]
        data["checkpoints"].append({"level": 0, "cols": [2, 4], "rows": [1, 4], "type": "individual", "number": 2})
        self.assertEqual([c["number"] for c in canonicalize_map(data)["checkpoints"]], [2, 5])

    def test_copies_get_fresh_numbers_and_renumbering_follows_zone_references(self):
        root = checkpoint_map(1)
        root["checkpoints"].append({"level": 0, "cols": [5, 6], "rows": [5, 6], "type": "individual", "number": 4})
        root["actor_spawn_zones"] = [zone(until_checkpoint=2)]
        room = checkpoint_map(2)
        room["actor_spawn_zones"] = [zone(until_checkpoint=4, on_checkpoint="destroy")]
        root["nested_geometry"] = {"room": room}
        self.assertEqual(
            [(name, entry["number"]) for name, entry in checkpoint_entries(root)],
            [(None, 1), ("room", 2), (None, 4)],
        )
        self.assertEqual(next_checkpoint_number(root), 5)

        block = {
            "checkpoints": [{"number": 1}, {"number": 3}, {"type": "individual"}],
            "actor_spawn_zones": [zone(until_checkpoint=1), zone(until_checkpoint=3)],
        }
        copied = number_checkpoint_copies(block, root)
        self.assertEqual([entry.get("number") for entry in copied["checkpoints"]], [5, 3, None])
        self.assertEqual(
            [entry["until_checkpoint"] for entry in copied["actor_spawn_zones"]],
            [5, 3],
            "a zone copied with its checkpoint follows the fresh number",
        )
        self.assertEqual(block["checkpoints"][0]["number"], 1, "the block is left alone")

        generated = copy.deepcopy(root)
        generated["nested_geometry"]["room_rotated"] = copy.deepcopy(room)
        generated["nested_maps"] = [nested("room_rotated", 0, [5, 5], [5, 5])]
        kept = number_generated_definitions(generated, {"room_rotated"})
        self.assertEqual(kept["nested_geometry"]["room_rotated"]["checkpoints"][0]["number"], 2, "unplaced original")
        generated["nested_maps"].append(nested("room", 0, [5, 1], [5, 1]))
        fresh = number_generated_definitions(generated, {"room_rotated"})
        rotated = fresh["nested_geometry"]["room_rotated"]
        self.assertEqual(rotated["checkpoints"][0]["number"], 5, "the placed original keeps 2")
        self.assertEqual(rotated["actor_spawn_zones"][0]["until_checkpoint"], 4, "an outside reference stands")
        self.assertEqual(fresh["nested_geometry"]["room"]["checkpoints"][0]["number"], 2)

        after = renumber_checkpoints(root, {1: 2, 2: 1, 4: 10})
        self.assertEqual(
            {(name, entry["number"]) for name, entry in checkpoint_entries(after)},
            {(None, 2), ("room", 1), (None, 10)},
        )
        self.assertEqual(after["actor_spawn_zones"][0]["until_checkpoint"], 1)
        self.assertEqual(after["nested_geometry"]["room"]["actor_spawn_zones"][0]["until_checkpoint"], 10)
        self.assertEqual(root["checkpoints"][0]["number"], 1, "the document is left alone")
        with self.assertRaisesRegex(ValueError, "unique"):
            renumber_checkpoints(root, {1: 4})
        with self.assertRaisesRegex(ValueError, "distinct"):
            renumber_checkpoints(root, {1: 0})

    def test_copy_paste_transform_levels_and_deletion_preserve_zone_semantics(self):
        data = checkpoint_map()
        region = TileRegion((1, 1, 4, 4), 0)
        block = copy_region(data, region)
        self.assertEqual(
            block["checkpoints"], [{"level": 0, "cols": [0, 3], "rows": [0, 3], "type": "individual", "number": 1}]
        )
        pasted = paste_region(delete_region(data, region), block, (4, 4), 0)
        self.assertEqual(
            pasted["checkpoints"], [{"level": 0, "cols": [4, 7], "rows": [4, 7], "type": "individual", "number": 1}]
        )
        with self.assertRaisesRegex(ValueError, "checkpoint"):
            copy_region(data, TileRegion((1, 1, 2, 2), 0))
        moved = translate_map(data, 1, 2)
        self.assertEqual(moved["checkpoints"][0]["rows"], [3, 6])
        resized = resize_map_data(data, 3, 3, 0, 0)
        self.assertEqual(resized["checkpoints"][0]["cols"], [1, 3])
        inserted = insert_level_data(data, 0)
        self.assertEqual(inserted["checkpoints"][0]["level"], 1)
        self.assertEqual(remove_level_data(inserted, 1)["checkpoints"], [])

    def test_checkpoint_erasing_does_not_erase_other_zones_or_floors(self):
        data = checkpoint_map()
        hit = hit_at(data, 0, 2.5, 2.5, 0.1)
        self.assertEqual(hit, (HIT_CHECKPOINT, ("checkpoints", 0)))
        self.assertEqual(erase_hit(data, 0, hit)["checkpoints"], [])
        data["player_spawn_zones"] = [{"level": 0, "cols": [1, 2], "rows": [1, 2]}]
        erased = erase_group_rect(data, MODE_ERASE_CHECKPOINTS, 0, (1, 1, 4, 4))
        self.assertEqual(erased["checkpoints"], [])
        self.assertEqual(erased["player_spawn_zones"], data["player_spawn_zones"])
        self.assertEqual(erased["levels"], data["levels"])
        self.assertEqual(
            erase_group_rect(data, MODE_ERASE_SPAWN_ZONES, 0, (1, 1, 4, 4))["checkpoints"], data["checkpoints"]
        )
        for keep_floors in (False, True):
            self.assertEqual(erase_cell_rect(data, 0, (1, 1), (3, 3), keep_floors)["checkpoints"], [])

    def test_types_survive_nested_roundtrips_and_region_operations(self):
        for kind in CHECKPOINT_TYPE_LABELS:
            data = checkpoint_map()
            data["checkpoints"][0]["type"] = kind
            data["nested_geometry"] = {"platform": copy.deepcopy(data)}
            with tempfile.TemporaryDirectory() as directory:
                path = Path(directory) / "layout.json"
                write_map(path, data)
                restored = read_map(path)
            self.assertEqual(restored["nested_geometry"]["platform"]["checkpoints"][0]["type"], kind)
            region = TileRegion((1, 1, 4, 4), 0)
            pasted = paste_region(delete_region(restored, region), copy_region(restored, region), (4, 4), 0)
            self.assertEqual(pasted["checkpoints"][0]["type"], kind)
            self.assertEqual(insert_level_data(restored, 0)["checkpoints"][0]["type"], kind)
            self.assertEqual(resize_map_data(restored, 4, 4, 0, 0)["checkpoints"][0]["type"], kind)

    def test_unknown_missing_and_overlapping_types_are_rejected(self):
        for kind in (None, "unknown"):
            data = checkpoint_map()
            if kind is None:
                del data["checkpoints"][0]["type"]
            else:
                data["checkpoints"][0]["type"] = kind
            self.assertTrue(any("checkpoint type" in error for error in validate_map(normalize_map(data), [], [])))
        data = checkpoint_map()
        data["checkpoints"].append({**data["checkpoints"][0], "type": "group_all", "number": 2})
        self.assertTrue(any("overlaps" in error for error in validate_map(canonicalize_map(data), [], [])))


class CheckpointWindowTests(WindowTestCase):
    def drag(self, start, end):
        canvas = self.window.canvas
        QTest.mousePress(canvas, Qt.MouseButton.LeftButton, pos=canvas.viewport.from_grid(QPointF(*start)).toPoint())
        QTest.mouseRelease(canvas, Qt.MouseButton.LeftButton, pos=canvas.viewport.from_grid(QPointF(*end)).toPoint())

    def test_toolbar_number_is_used_at_placement_advances_and_survives_undo_and_save(self):
        window = self.window
        data = checkpoint_map()
        data["checkpoints"] = []
        window.doc.replace_with_new(data)
        window.set_mode(MODE_CHECKPOINT)
        spin = window.tool_settings.findChild(QSpinBox)
        self.assertEqual(spin.value(), 1)
        spin.setValue(3)
        window.set_mode(MODE_SELECT)
        window.set_mode(MODE_CHECKPOINT)
        self.assertEqual(window.tool_settings.findChild(QSpinBox).value(), 3, "an unused number stands")
        self.drag((1.5, 1.5), (2.5, 2.5))
        expected = [{"level": 0, "cols": [1, 3], "rows": [1, 3], "type": "individual", "number": 3}]
        self.assertEqual(window.map_data["checkpoints"], expected)
        self.assertEqual(window.tool_settings.findChild(QSpinBox).value(), 4, "the next free number follows")
        self.assertEqual(window.undo_stack.count(), 1)
        window.undo_stack.undo()
        self.assertEqual(window.map_data["checkpoints"], [])
        window.undo_stack.redo()
        self.assertEqual(window.map_data["checkpoints"], expected)
        window.doc.write(self.path)
        self.assertEqual(read_map(self.path)["checkpoints"], expected)

    def test_a_used_number_is_refused_at_placement_until_changed(self):
        window = self.window
        data = checkpoint_map()
        data["checkpoints"] = []
        window.doc.replace_with_new(data)
        window.set_mode(MODE_CHECKPOINT)
        self.click(1, 1)
        spin = window.tool_settings.findChild(QSpinBox)
        self.assertEqual(spin.value(), 2)
        spin.setValue(1)
        before = copy.deepcopy(window.map_data)
        with patch.object(window, "notify") as notify:
            self.click(2, 1)
        self.assertIn("already in use", notify.call_args.args[0])
        self.assertEqual(window.map_data, before)
        self.assertEqual(window.undo_stack.count(), 1)
        spin.setValue(5)
        self.click(2, 1)
        self.click(3, 1)
        numbers = {tuple(zone["cols"]): zone["number"] for zone in window.map_data["checkpoints"]}
        self.assertEqual(numbers, {(1, 2): 1, (2, 3): 5, (3, 4): 6})
        self.assertEqual(window.undo_stack.count(), 3)
        self.assertFalse(window.validate(window.map_data))
        window.set_mode(MODE_SELECT)
        window.set_mode(MODE_CHECKPOINT)
        self.assertEqual(window.tool_settings.findChild(QSpinBox).value(), 7)

    def test_checkpoint_copies_get_fresh_numbers_and_moves_keep_numbers(self):
        window = self.window
        for objects in (False, True):
            for operation in ("copy", "cut", "duplicate", "move"):
                with self.subTest(objects=objects, operation=operation):
                    data = checkpoint_map()
                    data["levels"][0]["floors"] += [floor(c, r) for c in range(4, 7) for r in range(4, 7)]
                    window.apply_change("Set up checkpoint", data)
                    window.set_mode(MODE_SELECT)
                    if objects:
                        window.set_selected_spawn_zone(ZoneRef("checkpoints", 0))
                    else:
                        window.set_tile_selection((1, 1, 4, 4))
                    if operation in ("copy", "cut"):
                        getattr(window, f"{operation}_selection")()
                        window.set_tile_selection((4, 4, 5, 5))
                        window.paste_selection()
                    else:
                        self.assertTrue(window.begin_transfer(duplicate=operation == "duplicate"))
                        window.pending_block.destination = (4, 4)
                        window.commit_pending_block()
                        self.assertIsNone(window.pending_block)
                    expected = {1, 2} if operation in ("copy", "duplicate") else {1}
                    self.assertEqual({c["number"] for c in window.map_data["checkpoints"]}, expected)
                    self.assertEqual(len(window.map_data["checkpoints"]), len(expected))
                    self.assertFalse(window.added_issues(window.map_data))
                    window.undo_stack.undo()
                    window.undo_stack.redo()
                    self.assertEqual({c["number"] for c in window.map_data["checkpoints"]}, expected)

    def test_place_select_resize_undo_and_render(self):
        window = self.window
        data = checkpoint_map()
        data["checkpoints"] = []
        window.apply_change("Set up floors", data)
        window.set_mode(MODE_CHECKPOINT)
        self.click(1, 1)
        self.assertEqual(
            window.map_data["checkpoints"],
            [{"level": 0, "cols": [1, 2], "rows": [1, 2], "type": "individual", "number": 1}],
        )
        window.set_mode(MODE_SELECT)
        self.click(1, 1)
        self.drag((2, 2), (3, 3))
        self.assertEqual(window.map_data["checkpoints"][0]["cols"], [1, 3])
        window.undo_stack.undo()
        self.assertEqual(window.map_data["checkpoints"][0]["cols"], [1, 2])
        window.undo_stack.redo()
        self.assertTrue(window.canvas.grab().save(str(Path(self.temp.name) / "checkpoint.png")))
        window.erase_group_rect(MODE_ERASE_CHECKPOINTS, (1, 1), (3, 3))
        self.assertEqual(window.map_data["checkpoints"], [])
        self.assertIsNone(window.selected_spawn_zone_ref)

    def test_properties_edit_the_checkpoint_number_and_refuse_a_used_one(self):
        window = self.window
        data = checkpoint_map()
        data["checkpoints"][0]["cols"] = [1, 2]
        data["checkpoints"].append({"level": 0, "cols": [2, 4], "rows": [1, 4], "type": "individual", "number": 2})
        window.apply_change("Set up checkpoints", data)
        window.set_selected_spawn_zone(ZoneRef("checkpoints", 0))
        window.edit_selected_spawn_zone_fields()
        self.set_property("number", 5)
        window.properties_panel.apply_button.click()
        by_cols = {tuple(c["cols"]): c["number"] for c in window.map_data["checkpoints"]}
        self.assertEqual(by_cols, {(1, 2): 5, (2, 4): 2})
        index = next(i for i, c in enumerate(window.map_data["checkpoints"]) if c["number"] == 5)
        window.set_selected_spawn_zone(ZoneRef("checkpoints", index))
        window.edit_selected_spawn_zone_fields()
        self.set_property("number", 2)
        window.properties_panel.apply_button.click()
        self.assertIn("already used", window.properties_panel.error.text())
        self.assertEqual({c["number"] for c in window.map_data["checkpoints"]}, {2, 5})

    def test_toolbar_and_context_edit_checkpoint_type_with_undo(self):
        window = self.window
        data = checkpoint_map()
        data["checkpoints"] = []
        window.apply_change("Set up floors", data)
        window.set_mode(MODE_CHECKPOINT)
        box = next(box for box in window.tool_settings.findChildren(QComboBox) if box.accessibleName() == "Type")
        self.assertEqual(box.currentData(), "individual")
        box.setCurrentIndex(box.findData("group_any"))
        self.click(1, 1)
        self.assertEqual(window.map_data["checkpoints"][0]["type"], "group_any")
        window.set_selected_spawn_zone(ZoneRef("checkpoints", 0))
        self.assertTrue(window.selected_spawn_zone_has_fields())
        window.edit_selected_spawn_zone_fields()
        self.set_property("type", "group_all")
        window.properties_panel.apply_button.click()
        self.assertEqual(window.map_data["checkpoints"][0]["type"], "group_all")
        self.assertEqual(window.recent_checkpoint_type, "group_any")
        window.undo_stack.undo()
        self.assertEqual(window.map_data["checkpoints"][0]["type"], "group_any")
        window.undo_stack.redo()
        self.assertEqual(window.map_data["checkpoints"][0]["type"], "group_all")
        window.set_selected_spawn_zone(ZoneRef("checkpoints", 0))
        window.edit_selected_spawn_zone_fields()
        self.set_property("type", "individual")
        window.properties_panel.rebuild()
        self.assertEqual(window.map_data["checkpoints"][0]["type"], "group_all")

    def test_actor_zone_properties_edit_the_course_end(self):
        window = self.window
        data = checkpoint_map(2)
        data["actor_spawn_zones"] = [zone()]
        window.apply_change("Set up zone", data)
        panel = window.properties_panel
        window.set_selected_spawn_zone(ZoneRef("actor_spawn_zones", 0))
        window.edit_selected_spawn_zone_fields()
        self.assertEqual(panel.widgets[("until_checkpoint",)].text(), "Always")
        self.assertFalse(panel.widgets[("on_checkpoint",)].isEnabled())
        self.set_property("until_checkpoint", 2)
        self.assertTrue(panel.widgets[("on_checkpoint",)].isEnabled())
        self.set_property("on_checkpoint", "destroy")
        panel.apply_button.click()
        edited = window.map_data["actor_spawn_zones"][0]
        self.assertEqual((edited["until_checkpoint"], edited["on_checkpoint"]), (2, "destroy"))

        window.set_selected_spawn_zone(ZoneRef("actor_spawn_zones", 0))
        window.edit_selected_spawn_zone_fields()
        self.set_property("until_checkpoint", 9)
        panel.apply_button.click()
        self.assertIn("names no checkpoint", panel.error.text())
        self.set_property("until_checkpoint", "Always")
        panel.apply_button.click()
        edited = window.map_data["actor_spawn_zones"][0]
        self.assertNotIn("until_checkpoint", edited)
        self.assertNotIn("on_checkpoint", edited)

    def test_numbers_repeated_across_placed_definitions_are_reported(self):
        window = self.window
        root = checkpoint_map(1)
        root["nested_geometry"] = {"room": checkpoint_map(1)}
        window.doc.replace_with_new(root)
        self.assertEqual(window.validate_document(window.doc.root_data), [], "an unplaced definition is scratch")
        root["nested_maps"] = [nested("room", 0, [0, 0], [0, 0])]
        window.doc.replace_with_new(root)
        errors = window.validate_document(window.doc.root_data)
        self.assertTrue(any("also used" in error for error in errors), list(errors))
        root["nested_geometry"]["room"]["checkpoints"][0]["number"] = 2
        root["actor_spawn_zones"] = [zone(until_checkpoint=2)]
        window.doc.replace_with_new(root)
        self.assertEqual(window.validate_document(window.doc.root_data), [], "a zone may end at a nested checkpoint")

    def test_properties_refuse_numbers_other_placed_definitions_use_or_still_end_at(self):
        window = self.window
        root = checkpoint_map(1)
        room = checkpoint_map(2)
        room["actor_spawn_zones"] = [zone(until_checkpoint=1)]
        root["nested_geometry"] = {"room": room}
        root["nested_maps"] = [nested("room", 0, [0, 0], [0, 0])]
        window.doc.replace_with_new(root)
        self.assertEqual(window.validate_document(window.doc.root_data), [])
        panel = window.properties_panel
        for number, message in ((2, "already used in another"), (3, "still the end of an actor zone")):
            window.set_selected_spawn_zone(ZoneRef("checkpoints", 0))
            window.edit_selected_spawn_zone_fields()
            self.set_property("number", number)
            panel.apply_button.click()
            self.assertIn(message, panel.error.text())
            self.assertEqual(window.map_data["checkpoints"][0]["number"], 1)
        room["actor_spawn_zones"] = []
        window.doc.replace_with_new(root)
        window.set_selected_spawn_zone(ZoneRef("checkpoints", 0))
        window.edit_selected_spawn_zone_fields()
        self.set_property("number", 3)
        panel.apply_button.click()
        self.assertEqual(window.map_data["checkpoints"][0]["number"], 3)

    def test_transforming_nested_geometry_keeps_or_frees_its_checkpoint_numbers(self):
        window = self.window
        room = empty_map(2, 2)
        room["player_spawn_zones"] = []
        room["levels"][0]["floors"] = [floor(c, r) for c in range(2) for r in range(2)]
        room["checkpoints"] = [{"level": 0, "cols": [0, 2], "rows": [0, 2], "type": "individual", "number": 5}]
        room["actor_spawn_zones"] = [{**zone(until_checkpoint=5), "cols": [0, 1], "rows": [0, 1]}]
        for placed_twice in (False, True):
            with self.subTest(placed_twice=placed_twice):
                root = checkpoint_map(1)
                root["actor_spawn_zones"] = [zone(until_checkpoint=5)]
                root["nested_geometry"] = {"room": copy.deepcopy(room)}
                root["nested_maps"] = [nested("room", 0, [5, 1], [5, 1])]
                if placed_twice:
                    root["nested_maps"].append(nested("room", 0, [5, 4], [5, 4]))
                window.doc.replace_with_new(root)
                self.assertEqual(window.validate_document(window.doc.root_data), [])
                window.inspect_refs([ElementRef("nested_maps", 0)])
                window.transform_selection("rotate")
                self.assertTrue(window.pending_block.additions)
                window.pending_block.destination = (5, 1)
                with patch.object(window, "notify") as notify:
                    window.commit_pending_block()
                self.assertFalse(notify.called, notify.call_args)
                self.assertIsNone(window.pending_block)
                after = window.doc.root_data
                rotated = after["nested_geometry"]["room_rotated"]
                expected = 6 if placed_twice else 5
                self.assertEqual(rotated["checkpoints"][0]["number"], expected)
                self.assertEqual(rotated["actor_spawn_zones"][0]["until_checkpoint"], expected)
                self.assertEqual(after["nested_geometry"]["room"]["checkpoints"][0]["number"], 5)
                self.assertEqual(after["actor_spawn_zones"][0]["until_checkpoint"], 5)
                self.assertEqual(window.validate_document(after), [])

    def test_edit_checkpoints_dialog_renumbers_swaps_and_updates_zones(self):
        window = self.window
        root = checkpoint_map(1)
        root["checkpoints"][0]["cols"] = [1, 2]
        root["checkpoints"].append({"level": 0, "cols": [2, 4], "rows": [1, 4], "type": "individual", "number": 2})
        root["actor_spawn_zones"] = [zone(until_checkpoint=3)]
        room = checkpoint_map(3)
        room["actor_spawn_zones"] = [zone(until_checkpoint=2, on_checkpoint="destroy")]
        root["nested_geometry"] = {"room": room}
        root["nested_maps"] = [nested("room", 0, [0, 0], [0, 0])]
        window.doc.replace_with_new(root)
        self.assertEqual(window.validate_document(window.doc.root_data), [])
        before = copy.deepcopy(window.doc.root_data)

        def edit(dialog):
            self.assertEqual([row["map"] == "room" for row in dialog.rows], [False, False, True])
            dialog.table.setCurrentCell(1, 1)
            dialog.up_button.click()
            dialog.spin(2).setValue(10)
            dialog.accept()
            return dialog.result()

        with patch.object(CheckpointsDialog, "exec", edit):
            window.edit_checkpoints()
        after = window.doc.root_data
        self.assertEqual({tuple(c["cols"]): c["number"] for c in after["checkpoints"]}, {(1, 2): 2, (2, 4): 1})
        self.assertEqual(after["nested_geometry"]["room"]["checkpoints"][0]["number"], 10)
        self.assertEqual(after["actor_spawn_zones"][0]["until_checkpoint"], 10)
        self.assertEqual(after["nested_geometry"]["room"]["actor_spawn_zones"][0]["until_checkpoint"], 1)
        self.assertEqual(window.undo_stack.undoText(), "Edit Checkpoints")
        self.assertEqual(window.recent_checkpoint_number, 11)
        self.assertEqual(window.validate_document(after), [])
        window.undo_stack.undo()
        self.assertEqual(window.doc.root_data, before)

        def renumber(dialog):
            dialog.renumber_button.click()
            dialog.accept()
            return dialog.result()

        with patch.object(CheckpointsDialog, "exec", renumber):
            window.edit_checkpoints()
        self.assertEqual(window.doc.root_data, before, "consecutive numbers renumber nothing")
        self.assertEqual(window.undo_stack.count(), 1)

        def collide(dialog):
            dialog.spin(0).setValue(2)
            dialog.accept()
            return dialog.result()

        with (
            patch("map_editor.dialogs.checkpoints.QMessageBox.warning") as warning,
            patch.object(CheckpointsDialog, "exec", collide),
        ):
            window.edit_checkpoints()
        self.assertIn("unique", warning.call_args.args[2])
        self.assertEqual(window.doc.root_data, before)
