"""A test-owned course for the map authoring tools, on the fixture map "obby"."""

from map_author.builder import MapBuilder
from map_author.context import MapContext


# Two platforms a gap apart, a portal-ready wall and pad, a single-storey
# wall, a plate beside a pad, and one of everything else the plan draws.
def course():
    b = MapBuilder("obby", cols=12, rows=8, levels=3, solid="portal-resistant", portal="slab")
    b.level_name(1, "Deck")
    b.platform("west", level=1, at=(1, 2), size=(3, 2))
    b.checkpoint(0, level=1, at=(1, 2), size=(1, 1))
    b.platform("east", level=1, size=(2, 2), east_of="west", gap=3)
    b.portal_floor("pad", level=0, at=(2, 5))
    b.portal_floor("plated", level=0, at=(6, 5))
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
