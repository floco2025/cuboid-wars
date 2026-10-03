from pathlib import Path

from author_fixtures import FINE_GRID, LOW_STOREYS, pin_geometry
from config_fixtures import ConfigTestCase
from map_author.builder import BuildError, MapBuilder
from map_author.context import MapContext
from map_author.describe import summary
from map_author.frame import PortalFootprint
from map_author.index import MapIndex
from map_editor.io import read_map
from map_editor.normalization import edge_key


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
        pin_geometry(**LOW_STOREYS)
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

    def test_portal_wall_refuses_the_top_level_and_refaces_a_wall_already_there(self):
        pin_geometry(**LOW_STOREYS)
        b = builder()
        with self.assertRaises(BuildError):
            b.portal_wall("top", level=3, at=(5, 5), side="N")
        b.wall(level=1, start=(5, 5), end=(6, 5), material="basement-floor")
        b.portal_wall("refaced", level=1, at=(5, 5), side="N")
        (lower,) = b.data["levels"][1]["walls"]
        (upper,) = b.data["levels"][2]["walls"]
        self.assertEqual((lower["south"], lower["north"]), ("slab", "basement-floor"))
        self.assertEqual((upper["south"], upper["north"]), ("slab", "portal-resistant"))

    def test_a_strip_taller_than_a_portal_says_how_high_it_takes_one(self):
        pin_geometry(**LOW_STOREYS)
        b = builder(levels=6)
        b.portal_wall("strip", level=1, at=(5, 5), side="N")
        b.portal_wall("strip-top", level=3, at=(5, 5), side="N")
        text = summary(MapContext.load("obby", b.data))
        self.assertIn("wall  wall:L1:5,5:N  faces south", text)
        self.assertIn("ready from L1 up to L3", text)
        self.assertEqual(text.count("faces south"), 1)

    def test_portal_pieces_take_their_size_from_the_grid(self):
        tall = builder()
        self.assertEqual(tall.footprint, PortalFootprint(across=1, along=1, storeys=1))
        tall.portal_wall("gate", level=1, at=(5, 5), side="N")
        self.assertEqual([len(level["walls"]) for level in tall.data["levels"]], [0, 1, 0, 0])
        self.assertEqual(tall.portal_floor("pad", level=0, at=(2, 2)).size, (1, 1))

        pin_geometry(**FINE_GRID)
        fine = builder()
        self.assertEqual(fine.footprint, PortalFootprint(across=2, along=3, storeys=2))
        self.assertEqual(fine.portal_wall("gate", level=1, at=(5, 5), side="N").rect, (5, 5, 7, 6))
        self.assertEqual([len(level["walls"]) for level in fine.data["levels"]], [0, 2, 2, 0])
        self.assertEqual(fine.portal_floor("pad", level=0, at=(2, 2)).size, (3, 3))
        with self.assertRaisesRegex(BuildError, "2 cells wide"):
            fine.portal_wall("narrow", level=1, at=(9, 9), side="N", length=1)
        with self.assertRaisesRegex(BuildError, "3x2 cells"):
            fine.portal_floor("small", level=0, at=(9, 9), size=(2, 2))

    def test_a_fine_grid_needs_wider_backing_to_be_ready(self):
        pin_geometry(**FINE_GRID)
        b = builder()
        b.portal_wall("gate", level=1, at=(5, 5), side="N")
        b.portal_floor("long", level=0, at=(2, 2), size=(3, 2))
        b.platform("square", level=0, at=(8, 2), size=(2, 2), material="slab")
        index = MapIndex(MapContext.load("obby", b.data))
        self.assertEqual({s.reason for s in index.wall_surfaces(1)}, {""})
        b.data["levels"][1]["walls"].pop()
        b.data["levels"][2]["walls"].pop()
        narrow = MapIndex(MapContext.load("obby", b.data))
        self.assertEqual(
            {s.reason for s in narrow.wall_surfaces(1)}, {"1 cell wide: a portal needs 2 such sections side by side"}
        )
        pads = {min(s.cells): (s.axes, s.reason) for s in index.floor_surfaces(0)}
        self.assertEqual(pads[(2, 2)], ("x", ""))
        self.assertEqual(
            pads[(8, 2)], ("", "too small: a floor portal needs 3x2 portalable cells with no wall between")
        )

    def test_a_room_has_a_floor_walls_and_a_ceiling(self):
        b = builder()
        room = b.room(
            "hall", level=1, at=(4, 4), size=(4, 3), storeys=2, floor="slab", inside={"N": "slab"}, ceiling="slab"
        )
        self.assertEqual((room.rect, room.level, room.top_level), ((4, 4, 8, 7), 1, 3))
        self.assertEqual({f["top"] for f in b.data["levels"][1]["floors"]}, {"slab"})
        for level in (1, 2):
            walls = {edge_key(w): w for w in b.data["levels"][level]["walls"]}
            self.assertEqual(len(walls), 14)
            north, south = walls[(4, 4, 5, 4)], walls[(4, 7, 5, 7)]
            self.assertEqual((north["south"], north["north"]), ("slab", "portal-resistant"))
            self.assertEqual((south["north"], south["south"]), ("portal-resistant", "portal-resistant"))
        ceiling = b.data["levels"][3]["floors"]
        self.assertEqual(len(ceiling), 12)
        self.assertEqual({(f["bottom"], f["top"]) for f in ceiling}, {("slab", "portal-resistant")})
        self.assertEqual(b.data["levels"][2]["floors"], [])
        with self.assertRaisesRegex(BuildError, "no level"):
            b.room("tower", level=2, at=(12, 12), size=(2, 2), storeys=2)
        b.room("open", level=2, at=(12, 12), size=(2, 2), storeys=2, ceiling=False)
        with self.assertRaisesRegex(BuildError, "not in settings.json"):
            b.room("odd", level=0, at=(0, 0), size=(2, 2), floor="marble")

    def test_rooms_sharing_a_wall_or_a_slab_keep_each_side(self):
        b = builder()
        b.room("west", level=0, at=(2, 2), size=(3, 3), inside="slab", ceiling="slab")
        b.room("east", level=0, size=(3, 3), east_of="west", inside="basement-floor")
        walls = {edge_key(w): w for w in b.data["levels"][0]["walls"]}
        self.assertEqual(len(walls), 21)
        shared = walls[(5, 2, 5, 3)]
        self.assertEqual((shared["west"], shared["east"]), ("slab", "basement-floor"))
        b.room("upper", level=1, at=(2, 2), size=(3, 3), floor="basement-floor", ceiling=False)
        slabs = {(f["bottom"], f["top"]) for f in b.data["levels"][1]["floors"] if f["col"] < 5}
        self.assertEqual(slabs, {("slab", "basement-floor")})

    def test_a_doorway_opens_the_wall_and_takes_its_lights(self):
        b = builder()
        b.room("hall", level=0, at=(2, 2), size=(5, 4), storeys=2)
        b.light(level=0, at=(4, 5), side="S", kind="utility", height=2.0)
        b.doorway("hall", "S")
        self.assertNotIn((4, 6, 5, 6), {edge_key(w) for w in b.data["levels"][0]["walls"]})
        self.assertIn((4, 6, 5, 6), {edge_key(w) for w in b.data["levels"][1]["walls"]})
        self.assertEqual(b.data["levels"][0]["lights"], [])
        b.field("door")
        b.doorway("hall", "W", 1, width=2, storeys=2, field="door")
        b.doorway("hall", "N", 0, eraser=True)
        for level in (0, 1):
            self.assertEqual({edge_key(e) for e in b.data["levels"][level]["barriers"]}, {(2, 3, 2, 4), (2, 4, 2, 5)})
        self.assertEqual([edge_key(e) for e in b.data["levels"][0]["erasers"]], [(2, 2, 3, 2)])
        self.assertEqual(b.data["levels"][1]["erasers"], [])
        with self.assertRaisesRegex(BuildError, "leaves its 5-cell wall"):
            b.doorway("hall", "N", 4, width=2)
        with self.assertRaisesRegex(BuildError, "storeys tall"):
            b.doorway("hall", "E", storeys=3)
        b.platform("deck", level=0, at=(10, 10), size=(2, 2))
        with self.assertRaisesRegex(BuildError, "not a room"):
            b.doorway("deck", "N")

    def test_room_lights_keep_off_openings_and_portal_faces(self):
        b = builder()
        b.room("hall", level=0, at=(2, 2), size=(6, 6), inside={"N": "slab"})
        b.doorway("hall", "W", 2, width=2)
        self.assertEqual(b.room_lights("hall", "utility", every=2, portal_faces=False), 8)
        lights = {(light["col"], light["row"], light["side"]) for light in b.data["levels"][0]["lights"]}
        self.assertEqual(
            lights,
            {(3, 7, "S"), (5, 7, "S"), (7, 7, "S"), (7, 3, "E"), (7, 5, "E"), (7, 7, "E"), (2, 3, "W"), (2, 7, "W")},
        )
        self.assertEqual(b.room_lights("hall", "utility", every=2, portal_faces=False), 0)
        self.assertEqual(b.room_lights("hall", "utility", every=2), 3)
        with self.assertRaisesRegex(BuildError, "unknown light kind"):
            b.light(level=0, at=(3, 7), side="S", kind="neon", height=2.0)
        with self.assertRaisesRegex(BuildError, "No wall"):
            b.light(level=0, at=(4, 4), side="N", kind="utility", height=2.0)

    def test_a_walls_ends_and_edges_match_its_inside(self):
        b = builder()
        b.room("hall", level=0, at=(2, 2), size=(4, 4), inside="slab")
        north = {edge_key(w): w for w in b.data["levels"][0]["walls"]}[(2, 2, 3, 2)]
        self.assertEqual({north[face] for face in ("south", "east", "west", "top", "bottom")}, {"slab"})
        self.assertEqual(north["north"], "portal-resistant")

    def test_a_slabs_side_takes_the_face_of_the_wall_it_lies_in(self):
        b = builder()
        b.room("shaft", level=0, at=(2, 2), size=(4, 4), storeys=3, inside="slab")
        b.room("hall", level=1, size=(4, 4), east_of="shaft", inside="basement-floor")
        b.platform("deck", level=1, at=(2, 2), size=(4, 1))
        b.checkpoint(0, level="hall", at=(7, 3), size=(1, 1))
        root, _ = b.document()
        floors = {(f["col"], f["row"]): f for f in root["levels"][1]["floors"]}
        # The hall's floor meets the wall it shares with the shaft: from the shaft, that band is shaft wall.
        self.assertEqual(floors[(6, 3)]["west"], "slab")
        # The deck's free edge has no wall under it and keeps its own material.
        self.assertEqual(floors[(3, 2)]["south"], "portal-resistant")
        self.assertEqual(floors[(3, 2)]["north"], "portal-resistant")
        ceiling = {(f["col"], f["row"]): f for f in root["levels"][2]["floors"]}
        self.assertEqual(ceiling[(6, 3)]["west"], "slab")

    def test_lights_and_doorways_follow_the_storey_height(self):
        pin_geometry(level_height=1.6, floor_thickness=0.2)
        b = builder(levels=6)
        self.assertEqual(b.footprint.doorway, 2)
        b.room("hall", level=1, at=(2, 2), size=(6, 6), storeys=2)
        b.doorway("hall", "W")
        for level in (1, 2):
            self.assertNotIn((2, 4, 2, 5), {edge_key(w) for w in b.data["levels"][level]["walls"]})
        b.room_lights("hall", "utility", every=3)
        self.assertEqual([len(level["lights"]) for level in b.data["levels"][:4]], [0, 0, 8, 0])
        # A single 3.0 m wall hangs its lights at 1.875 m: on the upper storey, 0.275 m up it.
        self.assertEqual({light["height"] for light in b.data["levels"][2]["lights"]}, {0.275})
        b.light(1, (3, 7), "S", kind="utility", height=0.8)
        self.assertEqual(b.data["levels"][1]["lights"][-1]["height"], 0.8)
        with self.assertRaisesRegex(BuildError, "positive"):
            b.light(1, (4, 7), "S", kind="utility", height=0.0)

    def test_faces_change_one_material_of_what_stands_there(self):
        b = MapBuilder("obby", cols=12, rows=12, levels=3, solid="portal-resistant", portal="slab", default="slab")
        b.room("hall", level=0, at=(2, 2), size=(4, 4))
        b.face_wall(0, (2, 2), "N", "portal-resistant", length=2)
        b.face_slab(1, (2, 2), (2, 2), "portal-resistant", face="bottom")
        walls = {edge_key(w): w for w in b.data["levels"][0]["walls"]}
        self.assertEqual([walls[(c, 2, c + 1, 2)]["south"] for c in (2, 3, 4)], ["portal-resistant"] * 2 + ["slab"])
        ceiling = {(f["col"], f["row"]): f for f in b.data["levels"][1]["floors"]}
        self.assertEqual((ceiling[(3, 3)]["bottom"], ceiling[(3, 3)]["top"]), ("portal-resistant", "slab"))
        self.assertEqual(ceiling[(4, 4)]["bottom"], "slab")
        with self.assertRaisesRegex(BuildError, "no wall along"):
            b.face_wall(0, (3, 3), "N", "slab")
        with self.assertRaisesRegex(BuildError, "no slab under"):
            b.face_slab(2, (2, 2), (2, 2), "slab")

    def test_actor_zones_are_written_and_described(self):
        b = builder()
        b.platform("start", level=0, at=(2, 2), size=(4, 4))
        b.checkpoint(0, level="start", at=(2, 2), size=(2, 2))
        b.switch("alarm")
        b.plate(level=0, at=(5, 5), switch="alarm")
        b.actor_zone("turret", level="start", at=(4, 2), size=(2, 2), count=[1, 2], switch="alarm", initially_on=False)
        b.actor_zone("scuttler", level="start", at=(2, 4), size=(2, 2), respawn_secs=30, roam=6.0)
        with self.assertRaisesRegex(BuildError, "unknown actor kind"):
            b.actor_zone("dragon", level=0, at=(2, 2), size=(1, 1))
        root, warnings = b.document()
        self.assertEqual(warnings, [])
        self.assertEqual(
            root["actor_spawn_zones"][0],
            {
                "level": 0,
                "cols": [4, 6],
                "rows": [2, 4],
                "kind": "turret",
                "count": [1, 2],
                "respawn_secs": None,
                "switch": "alarm",
                "initially_on": False,
            },
        )
        text = summary(MapContext.load("obby", root))
        self.assertIn("actors turret x1/2 L0 cols 4..6 rows 2..4, no respawn, switch 'alarm', initially off", text)
        self.assertIn("actors scuttler x1 L0 cols 2..4 rows 4..6, respawn 30 s, roams 6 m", text)

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
