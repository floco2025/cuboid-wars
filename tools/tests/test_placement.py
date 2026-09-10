import copy
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from PySide6.QtCore import QPointF

from editor_fixtures import EditorHost, faces, floor, furnished_map, qt_app
from map_editor.document import MapDocument
from map_editor.editing import place_plate
from map_editor.normalization import empty_level, empty_map, pressure_plate_key
from map_editor.transforms import insert_level_data
from map_editor.validation import validate_map

BRIDGE_KIND = "skyway"


class PlacementTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        qt_app()

    def test_placing_a_bridge_rect_covers_every_dragged_cell(self) -> None:
        data = empty_map(3, 3)
        data["levels"][0]["floors"] = [floor(0, 0)]
        data["levels"][0]["inaccessible_floors"] = [floor(1, 0)]
        data["levels"].append(empty_level(1))
        data["ramps"] = [{"lower_level": 0, "low": [1, 1], "high": [3, 2], **faces()}]
        host = EditorHost(data, [BRIDGE_KIND])

        host.add_light_bridge_rect((0, 0), (2, 1), BRIDGE_KIND)

        self.assertEqual(
            {(b["col"], b["row"], b["kind"]) for b in host.map_data["levels"][0]["light_bridges"]},
            {(col, row, BRIDGE_KIND) for row in range(2) for col in range(3)},
        )
        self.assertEqual(host.statuses, [])
        errors = validate_map(host.map_data, [], [BRIDGE_KIND])
        self.assertTrue(any("[0, 0] sits on a floor" in e for e in errors))
        self.assertTrue(any("[1, 0] sits on a floor" in e for e in errors))
        self.assertTrue(any("[1, 1] sits on a ramp" in e for e in errors))

    def test_placing_on_an_occupied_cell_flashes_instead_of_removing(self) -> None:
        host = EditorHost(furnished_map(), ["bridge_1"])
        host.prompt_and_add_item(1, 1)
        self.assertTrue(host.statuses[-1].startswith("Item not placed"))
        host.add_pressure_plate(1, 1, "barrier_1")
        self.assertTrue(host.statuses[-1].startswith("Plate not placed"))
        host.add_light_at(QPointF(1.5, 1.05))
        self.assertIn("already a light", host.statuses[-1])
        self.assertEqual(len(host.map_data["items"]), 1)
        self.assertEqual(len(host.map_data["pressure_plates"]), 1)
        self.assertEqual(len(host.map_data["levels"][0]["lights"]), 1)

    def test_occupied_plate_tiles_reject_every_switch_but_allow_edit_and_undo(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            doc = MapDocument(None, recovery_dir=Path(directory))
            data = empty_map(8, 8)
            data["levels"][0]["floors"] = [floor(1, 1)]
            doc.replace_with_new(data)
            host = EditorHost(None, ["bridge"], doc=doc)
            host.switches = ["a", "b", "c", "fireworks"]
            host.add_pressure_plate(1, 1, "a")
            a = host.plates_at(1, 1)[0]
            before = copy.deepcopy(host.map_data)
            undo_count = doc.undo_stack.count()
            for place in (
                lambda: host.add_pressure_plate(1, 1, "a"),
                lambda: host.add_pressure_plate(1, 1, "b"),
                lambda: host.add_pressure_plate(1, 1, "fireworks"),
            ):
                place()
                self.assertIn("already a pressure plate", host.statuses[-1])
                self.assertEqual(host.map_data, before)
                self.assertEqual(doc.undo_stack.count(), undo_count)
            host.add_pressure_plate(2, 2, "void")
            self.assertEqual(host.statuses[-1], "Unknown switch 'void'")
            self.assertEqual(host.map_data, before)
            with patch("map_editor.placement.KindDialog.prompt", return_value="c"):
                host.edit_pressure_plate_at(pressure_plate_key(a))
            self.assertEqual([p["switch"] for p in host.plates_at(1, 1)], ["c"])
            doc.undo_stack.undo()
            self.assertEqual(host.map_data, before)
            upper = {**a, "level": 1, "switch": "fireworks"}
            upper_data = insert_level_data(host.map_data, 1)
            self.assertEqual(len(place_plate(upper_data, upper)["pressure_plates"]), 2)
