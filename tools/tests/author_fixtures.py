"""A test-owned course for the map authoring tools, on the fixture map "obby"."""

import json

from map_author.builder import MapBuilder
from map_author.context import MapContext
from map_editor import catalogs
from map_editor.catalogs import map_settings_path

# Obby's storeys: lower than a portal, so a wall portal needs two sections.
LOW_STOREYS = {"level_height": 2.4, "floor_thickness": 0.4}
# The fine grid PLAN.md asks for: a portal is two cells wide and three long.
FINE_GRID = {"grid_cell_size": 1.0, "level_height": 2.2, "floor_thickness": 0.2, "wall_thickness": 0.2}


# Portal backing depends on the grid, so a test about it says which grid it is on.
def pin_geometry(**geometry):
    path = map_settings_path("obby")
    settings = json.loads(path.read_text())
    settings["geometry"] = {**settings["geometry"], **geometry}
    path.write_text(json.dumps(settings))


# What the game-backed tools read about the shooter.
def pin_shooter(eye_height=1.6, cooldown_secs=0.1):
    gameplay = json.loads(catalogs.GAMEPLAY_PATH.read_text())
    gameplay["player"]["eye_height"] = eye_height
    gameplay["weapons"]["projectiles"] = {"cooldown_secs": cooldown_secs}
    catalogs.GAMEPLAY_PATH.write_text(json.dumps(gameplay))


# A fine-grid hall on level 1: a four-cell portal panel in its north wall, a
# portal pad, and a portalable ramp outside; `deck` adds a portalable slab above.
def hall(deck=False):
    pin_geometry(**FINE_GRID)
    pin_shooter()
    b = MapBuilder("obby", cols=20, rows=20, levels=4, solid="portal-resistant", portal="slab")
    b.room("hall", level=1, at=(2, 2), size=(10, 8), storeys=2)
    b.checkpoint(0, level="hall", at=(3, 5), size=(2, 2))
    b.portal_wall("north", level="hall", at=(5, 2), side="N", length=4)
    b.portal_floor("pad", level="hall", at=(8, 5))
    b.ramp("ramp", lower_level=0, at=(14, 2), size=(4, 2), direction="E", material="slab")
    if deck:
        b.platform("deck", level=2, at=(14, 10), size=(3, 3), material="slab")
    root, _ = b.document()
    return MapContext.load("obby", root)


# Two platforms a gap apart, a portal-ready wall and pad, a single-storey
# wall, a plate beside a pad, and one of everything else the plan draws.
def course():
    pin_geometry(**LOW_STOREYS)
    b = MapBuilder("obby", cols=12, rows=8, levels=3, solid="portal-resistant", portal="slab")
    b.level_name(1, "Deck")
    b.platform("west", level=1, at=(1, 2), size=(3, 2))
    b.checkpoint(0, level=1, at=(1, 2), size=(1, 1))
    b.platform("east", level=1, size=(2, 2), east_of="west", gap=3)
    b.portal_floor("pad", level=0, at=(2, 5), size=(2, 2))
    b.portal_floor("plated", level=0, at=(6, 5), size=(2, 2))
    b.plate(level=0, at=(6, 5), switch="s")
    b.portal_wall("gate", level=1, at=(3, 3), side="S")
    b.wall(level=1, start=(1, 2), end=(4, 2), material="slab")
    b.wall(level=1, start=(7, 2), end=(9, 2))
    b.switch("s")
    b.field("door", switch="s", initially_on=False)
    b.barrier(level=1, start=(4, 2), end=(4, 3), field="door")
    b.bridge("span", level=1, at=(4, 2), size=(3, 1), field="door")
    b.eraser(level=1, start=(7, 2), end=(7, 4))
    b.item("speed", level=1, at=(2, 3))
    b.ramp("up", lower_level=0, at=(9, 0), size=(2, 1), direction="E")
    b.ladder(lower_level=0, landing=(1, 2), side="S")
    b.fireworks("s")
    root, _ = b.document()
    return MapContext.load("obby", root)
