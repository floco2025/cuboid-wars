import json
import unittest

from editor_fixtures import actor_zone, floor, nested, saved_map
from map_editor.normalization import canonicalize_map, empty_map
from map_editor.validation import validate_map

FIELD = "treasure"
BRIDGE_FIELD = "skyway"


class FileIoTests(unittest.TestCase):
    def test_map_files_carry_the_field_catalog_and_no_schema_version(self) -> None:
        data = {"fireworks": None, **empty_map(2, 2)}
        data["fields"] = [
            {"id": FIELD, "color": "#ff3333"},
            {"id": BRIDGE_FIELD, "color": "#30d8ff", "switch": "door", "initially_on": False},
        ]
        data["levels"][0]["floors"] = [floor(0, 0)]
        data["levels"][0]["barriers"] = [{"c0": 0, "r0": 1, "c1": 1, "r1": 1, "field": FIELD}]
        data["levels"][0]["light_bridges"] = [{"col": 1, "row": 0, "field": BRIDGE_FIELD}]
        data["items"] = [{"level": 0, "col": 0, "row": 0, "type": "key", "field": FIELD}]

        text, loaded = saved_map(data)

        wrapper = json.loads(text)
        self.assertNotIn("version", wrapper)
        self.assertEqual(wrapper["map"]["fields"], data["fields"])
        self.assertEqual(wrapper["map"]["levels"][0]["barriers"], data["levels"][0]["barriers"])
        self.assertEqual(wrapper["map"]["levels"][0]["light_bridges"], data["levels"][0]["light_bridges"])
        self.assertEqual(wrapper["map"]["items"], data["items"])
        self.assertEqual(loaded, canonicalize_map(data))

    def test_a_light_round_trips_its_height(self) -> None:
        data = {"fireworks": None, **empty_map(2, 2)}
        data["levels"][0]["walls"] = [{"c0": 0, "r0": 0, "c1": 2, "r1": 0, "all": "wall"}]
        data["levels"][0]["lights"] = [
            {"col": 0, "row": 0, "side": "N", "kind": "utility", "height": 2.5},
            {"col": 1, "row": 0, "side": "N", "kind": "utility", "height": 1.9},
        ]
        text, loaded = saved_map(data)
        self.assertIn('"kind": "utility", "height": 1.9}', text)
        self.assertEqual(loaded["levels"][0]["lights"], data["levels"][0]["lights"])

    def test_plates_zones_and_nested_maps_round_trip_their_switches_and_initial_states(self) -> None:
        data = {"fireworks": None, **empty_map(2, 2)}
        data["levels"][0]["floors"] = [floor(0, 0), floor(1, 0)]
        data["pressure_plates"] = [
            {"level": 0, "col": 0, "row": 0, "switch": BRIDGE_FIELD},
            {"level": 0, "col": 1, "row": 0, "switch": "fireworks"},
        ]
        data["checkpoints"] = [{"level": 0, "cols": [0, 1], "rows": [0, 1], "type": "individual", "number": 1}]
        data["actor_spawn_zones"] = [
            actor_zone(switch=BRIDGE_FIELD, until_checkpoint=1, on_checkpoint="destroy"),
            actor_zone(count=[2]),
        ]
        data["nested_maps"] = [
            {**nested("tile", 0, [0, 0], [1, 0]), "switch": BRIDGE_FIELD, "initially_on": False},
            nested("tile", 0, [1, 1], [1, 1]),
        ]
        self.assertEqual(validate_map(data, [BRIDGE_FIELD], switches=[BRIDGE_FIELD, "fireworks"]), [])

        text, loaded = saved_map(data)

        self.assertEqual(text.count('"switch": "skyway"'), 3)
        self.assertEqual(loaded["pressure_plates"], data["pressure_plates"])
        self.assertEqual(loaded["actor_spawn_zones"], data["actor_spawn_zones"])
        self.assertEqual(loaded["checkpoints"], data["checkpoints"])
        self.assertEqual(loaded["nested_maps"], data["nested_maps"])

    def test_a_ramp_line_omits_its_defaults_and_round_trips_the_rest(self) -> None:
        data = empty_map(6, 6)
        data["levels"] += [dict(data["levels"][0]), dict(data["levels"][0])]
        data["ramps"] = [
            {"lower_level": 0, "cols": [1, 2], "rows": [1, 3], "direction": "S", "all": "test"},
            {
                "lower_level": 0,
                "levels": 2,
                "cols": [3, 4],
                "rows": [1, 5],
                "direction": "N",
                "shape": "plank",
                "all": "test",
            },
        ]
        text, loaded = saved_map(data)
        plain, tall = (line for line in text.splitlines() if '"direction"' in line)
        self.assertNotIn('"levels"', plain)
        self.assertNotIn('"shape"', plain)
        self.assertIn('"levels": 2', tall)
        self.assertIn('"shape": "plank"', tall)
        self.assertEqual([(r["levels"], r["shape"]) for r in loaded["ramps"]], [(1, "solid"), (2, "plank")])

    def test_nested_maps_round_trip_and_are_the_last_key(self) -> None:
        data = empty_map(6, 6)
        data["nested_maps"] = [
            {**nested("cabin", 0, [2, 2], [4, 2]), "from_nudge": [0.3, 0.0, 0.0], "to_nudge": [0.0, -1.0, 1.01]}
        ]
        text, loaded = saved_map(data)
        self.assertGreater(text.index('"nested_maps"'), text.index('"ramps"'))
        self.assertEqual(loaded["nested_maps"], data["nested_maps"])
