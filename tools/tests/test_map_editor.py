import json
import tempfile
import unittest
from pathlib import Path

from map_editor.constants import DEFAULT_ALIAS, FACES
from map_editor.geometry import ramp_axis, ramp_cells, wall_segments_between
from map_editor.io import empty_map, read_map, write_map
from map_editor.normalization import canonicalize_map, resize_map_data
from map_editor.validation import validate_map


def faces(alias: str = DEFAULT_ALIAS) -> dict[str, str]:
    return {face: alias for face in FACES}


def floor(col: int, row: int) -> dict:
    return {"col": col, "row": row, **faces()}


KIND = "treasure"


def map_with_kinds(cols: int, rows: int, kinds: list[str] = (KIND,)) -> dict:
    data = empty_map(cols, rows)
    data["barrier_kinds"] = list(kinds)
    return data


class GeometryTests(unittest.TestCase):
    def test_wall_segments_are_unit_length_and_canonical(self) -> None:
        self.assertEqual(
            wall_segments_between((3, 2), (0, 2)),
            [[2, 2, 3, 2], [1, 2, 2, 2], [0, 2, 1, 2]],
        )

    def test_ramp_cells_and_axis_follow_its_footprint(self) -> None:
        ramp = {"low": [3, 1], "high": [0, 3]}
        self.assertEqual(ramp_axis(ramp), "west")
        self.assertEqual(ramp_cells(ramp), {(0, 1), (1, 1), (2, 1), (0, 2), (1, 2), (2, 2)})


class NormalizationTests(unittest.TestCase):
    def test_canonicalization_deduplicates_edges_and_applies_ramp_floor_rules(self) -> None:
        data = empty_map(4, 4)
        data["levels"].append(
            {
                "name": "Upper",
                "floors": [floor(0, 0), floor(1, 0)],
                "inaccessible_floors": [],
                "grass": [],
                "walls": [],
                "barriers": [],
                "lights": [],
            }
        )
        data["levels"][0]["walls"] = [
            {"c0": 1, "r0": 1, "c1": 0, "r1": 1, **faces()},
            {"c0": 0, "r0": 1, "c1": 1, "r1": 1, **faces()},
        ]
        data["ramps"] = [{"lower_level": 0, "low": [0, 0], "high": [2, 1], **faces()}]

        result = canonicalize_map(data)

        self.assertEqual(len(result["levels"][0]["walls"]), 1)
        self.assertEqual(
            {(entry["col"], entry["row"]) for entry in result["levels"][0]["floors"]},
            {(0, 0), (1, 0)},
        )
        self.assertEqual(result["levels"][1]["floors"], [])


class FileIoTests(unittest.TestCase):
    def test_map_files_are_written_without_a_schema_version(self) -> None:
        data = empty_map(2, 2)
        data["levels"][0]["floors"] = [floor(0, 0)]

        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "map.json"
            write_map(path, data)

            wrapper = json.loads(path.read_text(encoding="utf-8"))
            self.assertNotIn("version", wrapper)
            self.assertEqual(read_map(path), canonicalize_map(data))


class ResizeTests(unittest.TestCase):
    def test_center_resize_translates_every_coordinate_family(self) -> None:
        data = map_with_kinds(4, 4)
        data["levels"].append(
            {
                "name": "Upper",
                "floors": [],
                "inaccessible_floors": [],
                "grass": [],
                "walls": [],
                "barriers": [],
                "lights": [],
            }
        )
        level = data["levels"][0]
        level["floors"] = [floor(1, 1)]
        level["walls"] = [{"c0": 1, "r0": 1, "c1": 2, "r1": 1, **faces()}]
        level["lights"] = [{"col": 1, "row": 1, "side": "N"}]
        data["actor_spawn_zones"] = [
            {"level": 0, "cols": [1, 3], "rows": [1, 3], "kind": "mine", "count": 1}
        ]
        data["items"] = [{"level": 0, "col": 1, "row": 1, "type": "cookie"}]
        data["pressure_plates"] = [{"level": 0, "col": 1, "row": 1, "type": "barrier", "kind": KIND}]
        data["ramps"] = [{"lower_level": 0, "low": [1, 1], "high": [3, 2], **faces()}]
        data["ladders"] = [{"lower_level": 0, "col": 1, "row": 1, "side": "N", "levels": 1}]

        result = resize_map_data(data, 6, 6, 1, 1)

        self.assertEqual((result["levels"][0]["floors"][0]["col"], result["levels"][0]["floors"][0]["row"]), (2, 2))
        wall = result["levels"][0]["walls"][0]
        self.assertEqual((wall["c0"], wall["r0"], wall["c1"], wall["r1"]), (2, 2, 3, 2))
        self.assertEqual(result["actor_spawn_zones"][0]["cols"], [2, 4])
        self.assertEqual((result["items"][0]["col"], result["items"][0]["row"]), (2, 2))
        self.assertEqual((result["pressure_plates"][0]["col"], result["pressure_plates"][0]["row"]), (2, 2))
        self.assertEqual(result["ramps"][0]["low"], [2, 2])
        self.assertEqual((result["ladders"][0]["col"], result["ladders"][0]["row"]), (2, 2))


class PressurePlateTests(unittest.TestCase):
    def test_canonicalization_keeps_one_plate_per_type_on_a_cell(self) -> None:
        data = map_with_kinds(2, 2)
        data["levels"][0]["floors"] = [floor(0, 0)]
        barrier = {"level": 0, "col": 0, "row": 0, "type": "barrier", "kind": KIND}
        firework = {"level": 0, "col": 0, "row": 0, "type": "firework"}
        data["pressure_plates"] = [firework, barrier, dict(firework)]

        result = canonicalize_map(data)

        self.assertEqual(result["pressure_plates"], [barrier, firework])

    def test_plates_round_trip_through_the_file_format(self) -> None:
        data = map_with_kinds(2, 2)
        data["levels"][0]["floors"] = [floor(0, 0), floor(1, 0)]
        data["pressure_plates"] = [
            {"level": 0, "col": 0, "row": 0, "type": "barrier", "kind": KIND},
            {"level": 0, "col": 1, "row": 0, "type": "firework"},
        ]

        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "map.json"
            write_map(path, data)
            text = path.read_text(encoding="utf-8")
            self.assertIn('"type": "firework"}', text)
            self.assertEqual(read_map(path)["pressure_plates"], data["pressure_plates"])

    def test_plate_validation_flags_bad_types_and_kinds(self) -> None:
        data = map_with_kinds(2, 2)
        data["levels"][0]["floors"] = [floor(0, 0)]
        data["pressure_plates"] = [
            {"level": 0, "col": 0, "row": 0, "type": "confetti"},
            {"level": 0, "col": 0, "row": 0, "type": "barrier", "kind": "nope"},
            {"level": 0, "col": 1, "row": 0, "type": "firework", "kind": KIND},
            {"level": 0, "col": 1, "row": 1, "type": "firework"},
            {"level": 0, "col": 1, "row": 1, "type": "firework"},
        ]

        errors = validate_map(data)

        self.assertTrue(any("unknown type 'confetti'" in error for error in errors))
        self.assertTrue(any("unknown barrier kind 'nope'; known: [treasure]" in error for error in errors))
        self.assertTrue(any("must not have `kind`" in error for error in errors))
        self.assertTrue(any("duplicates a plate" in error for error in errors))


class BarrierKindTests(unittest.TestCase):
    def test_unlisted_kind_is_flagged_naming_the_listed_ones(self) -> None:
        data = map_with_kinds(2, 2, [KIND, "lobby"])
        data["levels"][0]["floors"] = [floor(0, 0)]
        data["levels"][0]["barriers"] = [{"c0": 0, "r0": 0, "c1": 1, "r1": 0, "kind": "nope"}]
        data["items"] = [{"level": 0, "col": 0, "row": 0, "type": "key", "kind": "nope"}]

        errors = validate_map(data)

        self.assertTrue(any("barrier[0] has unknown kind 'nope'; known: [treasure, lobby]" in e for e in errors))
        self.assertTrue(any("unknown key kind 'nope'; known: [treasure, lobby]" in e for e in errors))

        data["barrier_kinds"] = []
        errors = validate_map(data)
        self.assertTrue(any("known: [(none listed)]" in e for e in errors))

    def test_listed_kind_needs_a_color_and_no_duplicates(self) -> None:
        data = map_with_kinds(2, 2, [KIND, KIND, "no_such_color", ""])
        data["levels"][0]["floors"] = [floor(0, 0)]

        errors = validate_map(data)

        self.assertTrue(any("barrier_kinds[1] duplicates 'treasure'" in e for e in errors))
        self.assertTrue(any("barrier_kinds[2] 'no_such_color' has no color" in e for e in errors))
        self.assertTrue(any("barrier_kinds[3] is empty" in e for e in errors))

    def test_barrier_kinds_round_trip_through_the_file_format(self) -> None:
        data = map_with_kinds(2, 2, [KIND, "lobby"])
        data["levels"][0]["floors"] = [floor(0, 0)]

        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "map.json"
            write_map(path, data)
            text = path.read_text(encoding="utf-8")
            self.assertIn('"barrier_kinds": ["treasure", "lobby"],', text)
            self.assertEqual(read_map(path)["barrier_kinds"], [KIND, "lobby"])
            self.assertEqual(read_map(path), canonicalize_map(data))


class ValidationTests(unittest.TestCase):
    def test_valid_minimal_map_has_no_errors(self) -> None:
        data = empty_map(2, 2)
        data["levels"][0]["floors"] = [floor(0, 0)]
        self.assertEqual(validate_map(data), [])

    def test_invalid_geometry_item_and_ladder_are_reported(self) -> None:
        data = empty_map(2, 2)
        data["levels"][0]["floors"] = [floor(0, 0)]
        data["levels"][0]["walls"] = [{"c0": 0, "r0": 0, "c1": 2, "r1": 0, **faces()}]
        data["items"] = [{"level": 0, "col": 1, "row": 1, "type": "cookie"}]
        data["ladders"] = [{"lower_level": 0, "col": 0, "row": 0, "side": "N", "levels": 1}]

        errors = validate_map(data)

        self.assertTrue(any("is not one grid edge" in error for error in errors))
        self.assertTrue(any("has no regular floor" in error for error in errors))
        self.assertTrue(any("but the map has 1 level(s)" in error for error in errors))


if __name__ == "__main__":
    unittest.main()
