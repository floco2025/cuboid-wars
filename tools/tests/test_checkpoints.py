import copy
import tempfile
import unittest
from pathlib import Path

from PySide6.QtCore import QPointF

from editor_fixtures import WindowTestCase, floor
from map_editor.constants import HIT_CHECKPOINT, MODE_CHECKPOINT, MODE_ERASE_CHECKPOINTS, MODE_ERASE_SPAWN_ZONES
from map_editor.erasing import erase_cell_rect, erase_group_rect, erase_hit, hit_at
from map_editor.io import read_map, write_map
from map_editor.normalization import empty_map, normalize_map
from map_editor.regions import TileRegion, copy_region, delete_region, paste_region
from map_editor.transforms import insert_level_data, remove_level_data, resize_map_data, translate_map
from map_editor.types import ZoneRef
from map_editor.validation import validate_map


def checkpoint_map():
    data = empty_map(8, 8)
    data["player_spawn_zones"] = []
    data["levels"][0]["floors"] = [floor(c, r) for c in range(1, 4) for r in range(1, 4)]
    data["checkpoints"] = [{"level": 0, "cols": [1, 4], "rows": [1, 4]}]
    return data


class CheckpointTests(unittest.TestCase):
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

    def test_copy_paste_transform_levels_and_deletion_preserve_zone_semantics(self):
        data = checkpoint_map()
        region = TileRegion((1, 1, 4, 4), 0)
        block = copy_region(data, region)
        self.assertEqual(block["checkpoints"], [{"level": 0, "cols": [0, 3], "rows": [0, 3]}])
        pasted = paste_region(delete_region(data, region), block, (4, 4), 0)
        self.assertEqual(pasted["checkpoints"], [{"level": 0, "cols": [4, 7], "rows": [4, 7]}])
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
        self.assertEqual(erase_group_rect(data, MODE_ERASE_SPAWN_ZONES, 0, (1, 1, 4, 4))["checkpoints"], data["checkpoints"])
        for keep_floors in (False, True):
            self.assertEqual(erase_cell_rect(data, 0, (1, 1), (3, 3), keep_floors)["checkpoints"], [])


class CheckpointWindowTests(WindowTestCase):
    def test_place_select_resize_undo_and_render(self):
        window = self.window
        data = checkpoint_map()
        data["checkpoints"] = []
        window.apply_change("Set up floors", data)
        window.mode_combo.setCurrentText(MODE_CHECKPOINT)
        self.click(1, 1)
        self.assertEqual(window.map_data["checkpoints"], [{"level": 0, "cols": [1, 2], "rows": [1, 2]}])
        window.set_selected_spawn_zone(ZoneRef("checkpoints", 0))
        self.assertTrue(window.begin_spawn_zone_drag(QPointF(2, 2)))
        window.update_spawn_zone_edit_drag(QPointF(3, 3))
        window.commit_spawn_zone_edit_drag()
        self.assertEqual(window.map_data["checkpoints"][0]["cols"], [1, 3])
        window.undo_stack.undo()
        self.assertEqual(window.map_data["checkpoints"][0]["cols"], [1, 2])
        window.undo_stack.redo()
        self.assertTrue(window.canvas.grab().save(str(Path(self.temp.name) / "checkpoint.png")))
        window.erase_group_rect(MODE_ERASE_CHECKPOINTS, (1, 1), (3, 3))
        self.assertEqual(window.map_data["checkpoints"], [])
        self.assertIsNone(window.selected_spawn_zone_ref)
