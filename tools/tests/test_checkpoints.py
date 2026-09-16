import copy
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from PySide6.QtCore import QPointF, Qt
from PySide6.QtTest import QTest
from PySide6.QtWidgets import QComboBox, QLineEdit

from editor_fixtures import EditorHost, WindowTestCase, floor
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
from map_editor.erasing import erase_cell_rect, erase_group_rect, erase_hit, hit_at
from map_editor.io import read_map, write_map
from map_editor.normalization import canonicalize_map, empty_map, normalize_map, zone_key
from map_editor.regions import TileRegion, copy_region, delete_region, paste_region
from map_editor.transforms import insert_level_data, remove_level_data, resize_map_data, translate_map
from map_editor.types import ZoneRef
from map_editor.validation import validate_map


def checkpoint_map():
    data = empty_map(8, 8)
    data["player_spawn_zones"] = []
    data["levels"][0]["floors"] = [floor(c, r) for c in range(1, 4) for r in range(1, 4)]
    data["checkpoints"] = [{"level": 0, "cols": [1, 4], "rows": [1, 4], "type": "individual"}]
    return data


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
        second = {**first, "type": "group_all"}
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
        data["nested_geometry"] = {"platform": checkpoint_map()}
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
            lambda d: d["checkpoints"].append(copy.deepcopy(d["checkpoints"][0])),
            lambda d: d["checkpoints"][0].update(level=2),
            lambda d: d["ramps"].append({"low": [1, 1], "high": [2, 3], "lower_level": 0}),
        ):
            bad = copy.deepcopy(data)
            mutate(bad)
            self.assertTrue(any("checkpoints" in error for error in validate_map(bad, [], [])))

    def test_names_are_kept_formatted_and_validated(self):
        data = checkpoint_map()
        data["checkpoints"][0]["name"] = "hall"
        self.assertEqual(normalize_map(data)["checkpoints"][0]["name"], "hall")
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "layout.json"
            write_map(path, data)
            self.assertIn('"name": "hall"', path.read_text())
            self.assertEqual(read_map(path)["checkpoints"][0]["name"], "hall")
        self.assertFalse(validate_map(data, [], []))
        for name, message in ((" hall", "surrounding spaces"), ("", "nonempty")):
            bad = copy.deepcopy(data)
            bad["checkpoints"][0]["name"] = name
            self.assertTrue(any(message in error for error in validate_map(bad, [], [])), name)
        data["checkpoints"][0]["cols"] = [1, 2]
        data["checkpoints"].append({"level": 0, "cols": [2, 4], "rows": [1, 4], "type": "individual", "name": "hall"})
        self.assertTrue(any("already used" in error for error in validate_map(data, [], [])))

    def test_copy_paste_transform_levels_and_deletion_preserve_zone_semantics(self):
        data = checkpoint_map()
        region = TileRegion((1, 1, 4, 4), 0)
        block = copy_region(data, region)
        self.assertEqual(block["checkpoints"], [{"level": 0, "cols": [0, 3], "rows": [0, 3], "type": "individual"}])
        pasted = paste_region(delete_region(data, region), block, (4, 4), 0)
        self.assertEqual(pasted["checkpoints"], [{"level": 0, "cols": [4, 7], "rows": [4, 7], "type": "individual"}])
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
        data["checkpoints"].append({**data["checkpoints"][0], "type": "group_all"})
        self.assertTrue(any("overlaps" in error for error in validate_map(canonicalize_map(data), [], [])))


class CheckpointWindowTests(WindowTestCase):
    def test_toolbar_name_is_used_at_placement_and_survives_undo_and_save(self):
        window = self.window
        data = checkpoint_map()
        data["checkpoints"] = []
        window.doc.replace_with_new(data)
        window.set_mode(MODE_CHECKPOINT)
        name = window.tool_settings.findChild(QLineEdit)
        name.setFocus()
        QTest.keyClicks(name, "  Upper hall  ")
        self.assertEqual(window.map_data["checkpoints"], [])
        window.set_mode(MODE_SELECT)
        window.set_mode(MODE_CHECKPOINT)
        self.assertEqual(window.tool_settings.findChild(QLineEdit).text(), "  Upper hall  ")
        canvas = window.canvas
        start = canvas.viewport.from_grid(QPointF(1.5, 1.5)).toPoint()
        end = canvas.viewport.from_grid(QPointF(2.5, 2.5)).toPoint()
        QTest.mousePress(canvas, Qt.MouseButton.LeftButton, pos=start)
        QTest.mouseRelease(canvas, Qt.MouseButton.LeftButton, pos=end)
        expected = [{"level": 0, "cols": [1, 3], "rows": [1, 3], "type": "individual", "name": "Upper hall"}]
        self.assertEqual(window.map_data["checkpoints"], expected)
        self.assertEqual(window.undo_stack.count(), 1)
        window.undo_stack.undo()
        self.assertEqual(window.map_data["checkpoints"], [])
        window.undo_stack.redo()
        self.assertEqual(window.map_data["checkpoints"], expected)
        window.doc.write(self.path)
        self.assertEqual(read_map(self.path)["checkpoints"], expected)

    def test_duplicate_placement_name_can_be_corrected_or_cleared_before_placing(self):
        window = self.window
        data = checkpoint_map()
        data["checkpoints"] = []
        window.doc.replace_with_new(data)
        window.set_mode(MODE_CHECKPOINT)
        name = window.tool_settings.findChild(QLineEdit)
        name.setText("hall")
        self.click(1, 1)
        before = copy.deepcopy(window.map_data)
        with patch.object(window, "notify") as notify:
            self.click(2, 1)
        self.assertIn("already in use", notify.call_args.args[0])
        self.assertEqual(window.map_data, before)
        self.assertEqual(window.undo_stack.count(), 1)
        self.assertEqual(name.text(), "hall")
        name.setText("landing")
        self.click(2, 1)
        name.setText("   ")
        self.click(3, 1)
        names = {tuple(zone["cols"]): zone.get("name") for zone in window.map_data["checkpoints"]}
        self.assertEqual(names, {(1, 2): "hall", (2, 3): "landing", (3, 4): None})
        self.assertEqual(window.undo_stack.count(), 3)
        self.assertFalse(window.validate(window.map_data))

    def test_named_checkpoint_copies_get_fresh_names_and_moves_keep_names(self):
        window = self.window
        for objects in (False, True):
            for operation in ("copy", "cut", "duplicate", "move"):
                with self.subTest(objects=objects, operation=operation):
                    data = checkpoint_map()
                    data["checkpoints"][0]["name"] = "hall"
                    data["levels"][0]["floors"] += [floor(c, r) for c in range(4, 7) for r in range(4, 7)]
                    window.apply_change("Set up named checkpoint", data)
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
                    expected = {"hall", "hall 2"} if operation in ("copy", "duplicate") else {"hall"}
                    self.assertEqual({c["name"] for c in window.map_data["checkpoints"]}, expected)
                    self.assertEqual(len(window.map_data["checkpoints"]), len(expected))
                    self.assertFalse(window.added_issues(window.map_data))
                    window.undo_stack.undo()
                    window.undo_stack.redo()
                    self.assertEqual({c["name"] for c in window.map_data["checkpoints"]}, expected)

    def test_place_select_resize_undo_and_render(self):
        window = self.window
        data = checkpoint_map()
        data["checkpoints"] = []
        window.apply_change("Set up floors", data)
        window.set_mode(MODE_CHECKPOINT)
        self.click(1, 1)
        self.assertEqual(
            window.map_data["checkpoints"], [{"level": 0, "cols": [1, 2], "rows": [1, 2], "type": "individual"}]
        )
        window.set_mode(MODE_SELECT)
        self.click(1, 1)
        canvas = window.canvas
        start = canvas.viewport.from_grid(QPointF(2, 2)).toPoint()
        end = canvas.viewport.from_grid(QPointF(3, 3)).toPoint()
        QTest.mousePress(canvas, Qt.MouseButton.LeftButton, pos=start)
        QTest.mouseRelease(canvas, Qt.MouseButton.LeftButton, pos=end)
        self.assertEqual(window.map_data["checkpoints"][0]["cols"], [1, 3])
        window.undo_stack.undo()
        self.assertEqual(window.map_data["checkpoints"][0]["cols"], [1, 2])
        window.undo_stack.redo()
        self.assertTrue(window.canvas.grab().save(str(Path(self.temp.name) / "checkpoint.png")))
        window.erase_group_rect(MODE_ERASE_CHECKPOINTS, (1, 1), (3, 3))
        self.assertEqual(window.map_data["checkpoints"], [])
        self.assertIsNone(window.selected_spawn_zone_ref)

    def test_properties_edit_and_clear_the_checkpoint_name(self):
        window = self.window
        window.apply_change("Set up checkpoint", checkpoint_map())
        for name, expected in (("hall", "hall"), ("", None)):
            window.set_selected_spawn_zone(ZoneRef("checkpoints", 0))
            window.edit_selected_spawn_zone_fields()
            self.set_property("name", name)
            window.properties_panel.apply_button.click()
            self.assertEqual(window.map_data["checkpoints"][0].get("name"), expected)

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
