import json
import tempfile
import unittest
from pathlib import Path

from editor_fixtures import DEFAULT_ALIAS, EditorHost, qt_app
from map_editor.constants import HIT_EQUIPMENT_ERASER, MODE_ERASE_EQUIPMENT_ERASERS
from map_editor.document import MapDocument
from map_editor.editing import paint_erasers
from map_editor.erasing import erase_group_rect, erase_hit, hit_at
from map_editor.formatting import format_map_file
from map_editor.normalization import canonicalize_map, empty_map, normalize_map
from map_editor.transforms import resize_map_data
from map_editor.validation import validate_map


class EquipmentTests(unittest.TestCase):
    def test_projectile_pickups_round_trip_through_the_map_editor(self):
        data = empty_map(4, 4)
        data["player_spawn_zones"] = []
        data["levels"][0]["floors"] = [{"col": col, "row": 1, "all": DEFAULT_ALIAS} for col in (1, 2)]
        data["items"] = [
            {"level": 0, "col": col, "row": 1, "type": kind}
            for col, kind in ((1, "single_shot"), (2, "multi_shot"))
        ]
        data = canonicalize_map(data)
        encoded = format_map_file({"map": data})
        self.assertEqual(normalize_map(json.loads(encoded)["map"]), data)
        self.assertFalse(validate_map(data, [], []))

    def test_fields_and_gun_round_trip_and_resize(self):
        data = empty_map(4, 4)
        data["player_spawn_zones"] = []
        data["levels"][0]["floors"] = [{"col": 1, "row": 1, "all": DEFAULT_ALIAS}]
        data["items"] = [{"level": 0, "col": 1, "row": 1, "type": "portal_gun"}]
        data = canonicalize_map(paint_erasers(data, 0, (1, 0), (1, 3)))
        encoded = format_map_file({"map": data})
        self.assertEqual(normalize_map(json.loads(encoded)["map"]), data)
        self.assertFalse(validate_map(data, [], []))
        moved = resize_map_data(data, 6, 6, 2, 2)
        self.assertEqual(moved["items"][0]["col"], 3)
        self.assertEqual(moved["levels"][0]["erasers"][0], {"c0": 3, "r0": 2, "c1": 3, "r1": 3})

    def test_picking_and_group_erasure_preserve_other_elements(self):
        data = paint_erasers(empty_map(4, 4), 0, (1, 0), (1, 3))
        hit = hit_at(data, 0, 1.0, 0.5, 0.2)
        self.assertEqual(hit, (HIT_EQUIPMENT_ERASER, (1, 0, 1, 1)))
        self.assertEqual(len(erase_hit(data, 0, hit)["levels"][0]["erasers"]), 2)
        after = erase_group_rect(data, MODE_ERASE_EQUIPMENT_ERASERS, 0, (0, 0, 4, 4))
        self.assertEqual(after["levels"][0]["erasers"], [])
        self.assertEqual(after["player_spawn_zones"], data["player_spawn_zones"])

    def test_invalid_and_duplicate_fields_are_reported_without_losing_records_on_load(self):
        data = empty_map(4, 4)
        data["levels"][0]["erasers"] = [
            {"c0": 1, "r0": 0, "c1": 1, "r1": 1},
            {"c0": 1, "r0": 1, "c1": 1, "r1": 0},
            {"c0": -1, "r0": 0, "c1": 1, "r1": 2},
        ]
        normalized = normalize_map(data)
        self.assertEqual(len(normalized["levels"][0]["erasers"]), 3)
        errors = "\n".join(validate_map(normalized, [], []))
        self.assertIn("duplicates another eraser", errors)
        self.assertIn("not one grid edge", errors)
        self.assertIn("outside the grid-line bounds", errors)

    def test_field_tool_edits_undo_and_redo_through_the_document(self):
        qt_app()
        with tempfile.TemporaryDirectory() as directory:
            doc = MapDocument(None, recovery_dir=Path(directory))
            data = empty_map(8, 8)
            data["levels"][0]["floors"] = [{"col": 1, "row": 1, "all": DEFAULT_ALIAS}]
            doc.replace_with_new(data)
            host = EditorHost(None, [], doc=doc)
            host.add_equipment_eraser_line((1, 1), (2, 1))
            self.assertEqual(len(host.map_data["levels"][0]["erasers"]), 1)
            doc.undo_stack.undo()
            self.assertEqual(host.map_data["levels"][0]["erasers"], [])
            doc.undo_stack.redo()
            self.assertEqual(len(host.map_data["levels"][0]["erasers"]), 1)
