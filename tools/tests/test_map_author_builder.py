from pathlib import Path

from config_fixtures import ConfigTestCase
from map_author.builder import BuildError, MapBuilder
from map_editor.io import read_map


def builder(levels=4):
    return MapBuilder("obby", cols=20, rows=20, levels=levels, solid="portal-resistant", portal="slab")


class MapBuilderTests(ConfigTestCase):
    def test_textures_must_match_their_portal_roles(self):
        with self.assertRaises(BuildError):
            MapBuilder("obby", cols=4, rows=4, levels=1, solid="slab", portal="slab")
        with self.assertRaises(BuildError):
            MapBuilder("obby", cols=4, rows=4, levels=1, solid="portal-resistant", portal="missing")

    def test_relative_placement_resolves_against_named_pieces(self):
        b = builder()
        a = b.platform("a", level=1, at=(4, 6), size=(3, 2))
        east = b.platform("east", level="a", up=1, size=(2, 2), east_of="a", gap=3, shift=1)
        west = b.platform("west", level=1, size=(2, 2), west_of="a", gap=1)
        south = b.platform("south", level=1, size=(1, 1), south_of="a", gap=2, shift=2)
        north = b.platform("north", level=1, size=(3, 1), north_of="a", gap=0)
        self.assertEqual((a.rect, a.level), ((4, 6, 7, 8), 1))
        self.assertEqual((east.rect, east.level), ((10, 7, 12, 9), 2))
        self.assertEqual(west.rect, (1, 6, 3, 8))
        self.assertEqual(south.rect, (6, 10, 7, 11))
        self.assertEqual(north.rect, (4, 5, 7, 6))
        floors = {(f["col"], f["row"]) for f in b.data["levels"][2]["floors"]}
        self.assertEqual(floors, {(10, 7), (11, 7), (10, 8), (11, 8)})

    def test_placement_needs_exactly_one_anchor(self):
        b = builder()
        b.platform("a", level=0, at=(0, 0), size=(2, 2))
        with self.assertRaises(BuildError):
            b.platform("b", level=0, size=(1, 1))
        with self.assertRaises(BuildError):
            b.platform("c", level=0, at=(5, 5), size=(1, 1), east_of="a")
        with self.assertRaises(BuildError):
            b.platform("d", level=0, size=(1, 1), east_of="a", west_of="a")
        with self.assertRaises(BuildError):
            b.platform("e", level=0, size=(1, 1), east_of="nowhere")
        with self.assertRaises(BuildError):
            b.platform("f", level=0, at=(19, 19), size=(2, 2))

    def test_portal_wall_writes_both_storeys_facing_the_cell(self):
        b = builder()
        piece = b.portal_wall("gate", level=1, at=(5, 5), side="W", length=2)
        self.assertEqual((piece.rect, piece.face), ((5, 5, 6, 7), "east"))
        for level in (1, 2):
            walls = {
                (w["c0"], w["r0"], w["c1"], w["r1"], w["east"], w["west"]) for w in b.data["levels"][level]["walls"]
            }
            self.assertEqual(
                walls, {(5, 5, 5, 6, "slab", "portal-resistant"), (5, 6, 5, 7, "slab", "portal-resistant")}
            )
        self.assertEqual(b.data["levels"][3]["walls"], [])

    def test_portal_wall_refuses_the_top_level_and_a_solid_edge(self):
        b = builder()
        with self.assertRaises(BuildError):
            b.portal_wall("top", level=3, at=(5, 5), side="N")
        b.wall(level=1, start=(5, 5), end=(6, 5))
        with self.assertRaises(BuildError):
            b.portal_wall("blocked", level=1, at=(5, 5), side="N")

    def test_save_writes_the_canonical_layout_the_editor_reads(self):
        b = builder()
        b.platform("start", level=0, at=(2, 2), size=(2, 2))
        b.checkpoint(0, level="start", at=(2, 2), size=(2, 2))
        b.switch("gate", activation="momentary", reset="never")
        b.field("door", switch="gate", initially_on=True)
        b.plate(level=0, at=(3, 3), switch="gate")
        b.barrier(level=0, start=(4, 2), end=(4, 4), field="door")
        b.item("speed", level=0, at=(2, 3))
        b.fireworks("gate")
        path = Path(self.temp.name) / "obby" / "layout.json"
        warnings = b.save(path, quiet=True)
        self.assertEqual(warnings, [])
        root, _ = b.document()
        self.assertEqual(read_map(path), root)
        self.assertEqual(root["fireworks"], {"switch": "gate", "cooldown_secs": 0.0})
        self.assertEqual(root["checkpoints"][0]["type"], "individual")

    def test_validation_errors_name_the_offending_record(self):
        b = builder()
        b.checkpoint(0, level=0, at=(2, 2), size=(2, 2))
        with self.assertRaisesRegex(BuildError, "requires flat accessible floor"):
            b.save(Path(self.temp.name) / "obby" / "layout.json", quiet=True)

    def test_a_steep_ramp_is_refused_unless_allowed(self):
        b = builder()
        with self.assertRaisesRegex(BuildError, "exceeds"):
            b.ramp("steep", lower_level=0, at=(2, 2), size=(1, 1), direction="E")
        b.ramp("steep", lower_level=0, at=(2, 2), size=(1, 1), direction="E", allow_steep=True)
        b.ramp("gentle", lower_level=0, at=(2, 6), size=(2, 1), direction="E")
        self.assertEqual([r["cols"] for r in b.data["ramps"]], [[2, 3], [2, 4]])

    def test_items_and_plates_follow_the_placement_rules(self):
        b = builder()
        b.platform("deck", level=0, at=(0, 0), size=(3, 3))
        b.ramp("ramp", lower_level=0, at=(5, 5), size=(2, 1), direction="E")
        with self.assertRaises(BuildError):
            b.item("speed", level=0, at=(5, 5))
        with self.assertRaises(BuildError):
            b.item("key", level=0, at=(1, 1))
        with self.assertRaises(BuildError):
            b.item("gold", level=0, at=(1, 1), field="x")
        b.item("key", level=0, at=(1, 1), field="door")
        with self.assertRaises(BuildError):
            b.plate(level=0, at=(8, 8), switch="s")
        b.plate(level=0, at=(2, 2), switch="s")
        with self.assertRaises(BuildError):
            b.plate(level=0, at=(2, 2), switch="s")

    def test_warnings_are_returned_rather_than_raised(self):
        b = builder()
        b.platform("start", level=0, at=(2, 2), size=(2, 2))
        b.checkpoint(0, level=0, at=(2, 2), size=(2, 2))
        b.switch("lonely")
        b.field("door", switch="lonely")
        warnings = b.save(Path(self.temp.name) / "obby" / "layout.json", quiet=True)
        self.assertEqual(len(warnings), 1)
        self.assertIn("lonely", warnings[0])
