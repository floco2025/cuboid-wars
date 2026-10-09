import json
import tempfile
import unittest
from pathlib import Path

from editor_fixtures import DEFAULT_ALIAS, EditorHost, blank_map, qt_app
from map_editor.constants import HIT_EQUIPMENT_ERASER, MODE_ERASE_EQUIPMENT_ERASERS
from map_editor.document import MapDocument
from map_editor.editing import paint_erasers
from map_editor.erasing import erase_group_rect, erase_hit, hit_at
from map_editor.formatting import format_map_file
from map_editor.normalization import canonicalize_map, empty_map, normalize_map
from map_editor.transforms import resize_map_offset
from map_editor.validation import validate_map


class EquipmentTests(unittest.TestCase):
    def test_pickups_and_erasers_round_trip_validate_and_follow_a_resize(self):
        data = {"fireworks": None, **blank_map(4, 4)}
        data["levels"][0]["floors"] = [{"col": col, "row": 1, "all": DEFAULT_ALIAS} for col in (1, 2, 3)]
        data["items"] = [
            {"level": 0, "col": col, "row": row, "type": kind}
            for col, row, kind in (
                (1, 1, "single_shot"),
                (2, 1, "multi_shot"),
                (3, 1, "portal_gun"),
                # An eraser pickup hangs in the air without a floor under it.
                (2, 2, "equipment_eraser"),
            )
        ]
        data = canonicalize_map(paint_erasers(data, 0, (1, 0), (1, 3)))
        encoded = format_map_file({"map": data})
        self.assertEqual(normalize_map(json.loads(encoded)["map"]), data)
        self.assertFalse(validate_map(data, []))
        moved = resize_map_offset(data, 6, 6, 2, 2)
        self.assertEqual([item["col"] for item in moved["items"]], [item["col"] + 2 for item in data["items"]])
        self.assertEqual(moved["levels"][0]["erasers"][0], {"c0": 3, "r0": 2, "c1": 3, "r1": 3})

    def test_picking_and_group_erasure_preserve_other_elements(self):
        data = paint_erasers(empty_map(4, 4), 0, (1, 0), (1, 3))
        hit = hit_at(data, 0, 1.0, 0.5, 0.2)
        self.assertEqual(hit, (HIT_EQUIPMENT_ERASER, (1, 0, 1, 1)))
        self.assertEqual(len(erase_hit(data, 0, hit)["levels"][0]["erasers"]), 2)
        after = erase_group_rect(data, MODE_ERASE_EQUIPMENT_ERASERS, 0, (0, 0, 4, 4))
        self.assertEqual(after["levels"][0]["erasers"], [])
        self.assertEqual(after["checkpoints"], data["checkpoints"])

    def test_invalid_and_duplicate_erasers_are_reported_without_losing_records_on_load(self):
        data = {"fireworks": None, **empty_map(4, 4)}
        data["levels"][0]["erasers"] = [
            {"c0": 1, "r0": 0, "c1": 1, "r1": 1},
            {"c0": 1, "r0": 1, "c1": 1, "r1": 0},
            {"c0": -1, "r0": 0, "c1": 1, "r1": 2},
        ]
        normalized = normalize_map(data)
        self.assertEqual(len(normalized["levels"][0]["erasers"]), 3)
        errors = "\n".join(validate_map(normalized, []))
        self.assertIn("duplicates another eraser", errors)
        self.assertIn("not one grid edge", errors)
        self.assertIn("outside the grid-line bounds", errors)

    def test_eraser_tool_edits_undo_and_redo_through_the_document(self):
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
