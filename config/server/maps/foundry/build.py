#!/usr/bin/env python3
"""Foundry: a hub, three courses that each chain a few portal decisions, and an exit they open."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[4] / "tools"))
from map_author.builder import MapBuilder  # noqa: E402

# Brick, tile, and plaster take a portal; metal does not. A storey is 1.6 m:
# a jump clears one and not two, a room is two tall, and a portal fills two.
b = MapBuilder("foundry", cols=48, rows=44, levels=12, solid="metal", portal="brick", default="brick")
# The hub stands on level 3, so a course can sink below it.
GROUND = 3
for level, name in {0: "Pits", GROUND: "Ground", 8: "Gallery"}.items():
    b.level_name(level, name)

# A course's plate latches: crossing it again on the way out undoes nothing.
b.switch("gallery", activation="latch", reset="never", color="#00ccff")
b.switch("slopes", activation="latch", reset="never", color="#ffcc00")
b.switch("finish", activation="momentary", reset="never", color="#33dd66")
b.field("gallery", switch="gallery", initially_on=True)
b.field("gallery-door", switch="gallery", initially_on=True, color="#00ccff")
b.field("slopes", switch="slopes", initially_on=True)
b.field("slopes-door", switch="slopes", initially_on=True, color="#ffcc00")

METAL = dict(floor="metal", inside="metal", outside="metal", ceiling="metal")
HUB = dict(floor="hub-floor", inside="hub-wall", outside="outside", ceiling="ceiling")

# Course 1, the Gallery. The hall is eight storeys tall with a chasm across
# its floor and the goal on a ledge at its east end, two storeys above the
# chasm's floor, so nothing in it climbs out. Only a band high on its west
# wall takes a portal, shot from the chasm's floor, a jump up through a door
# from the shaft's floor, or through the gallery's window, a storey tall so
# no body falls through. The speed comes from the shaft beside
# the hall, whose floor takes a portal and is seen best from the landing by
# the hub's door: a running jump off the landing into it throws a body out of
# the band into the chasm, while one off the gallery, five storeys up and
# reached by ladder, carries it across. The run comes out of the band as lift.
b.room("hall", level=2, at=(10, 6), size=(26, 12), storeys=8, **METAL)
b.face_wall(7, (10, 6), "W", "brick", length=12, storeys=2)
b.platform("ledge", level=4, at=(32, 6), size=(4, 12), material="metal")
b.wall(level=2, start=(32, 6), end=(32, 18), material="metal", storeys=2)
b.plate(level="ledge", at=(33, 12), switch="gallery")

b.room(
    "shaft",
    level=1,
    at=(2, 18),
    size=(18, 10),
    storeys=9,
    floor="floor",
    inside="metal",
    outside="metal",
    ceiling="metal",
)
b.platform("landing", level=GROUND, at=(16, 21), size=(4, 7), material="floor")
b.platform("gallery", level=GROUND + 5, at=(11, 18), size=(9, 3), material="floor")
b.ladder(lower_level=GROUND, landing=(18, 20), side="S", levels=5)
b.doorway("shaft", "N", 13, width=4, storeys=1, storey=7)
# A fall into the chasm walks out, drops to the shaft's floor, and climbs
# back to the landing.
b.doorway("hall", "S", 1, width=2, storeys=2)
b.ladder(lower_level=1, landing=(16, 27), side="W", levels=2)

# The hub: the start, a door to each course, and the corridor out.
b.room("hub", level=GROUND, at=(20, 20), size=(12, 12), storeys=2, **HUB)
b.checkpoint(0, level="hub", at=(25, 25), size=(2, 2))
b.doorway("hub", "W", 3, width=2)

# The ledge's way back: a door barred until the plate is pressed, which
# shows the goal from the hub, and a ramp up to the hub's floor.
b.room("passage", level=GROUND, at=(32, 18), size=(3, 4), storeys=3, **HUB)
b.ramp("steps", lower_level=GROUND, at=(33, 18), size=(2, 2), direction="N", material="hub-floor")
b.doorway("hall", "S", 23, width=2, storeys=2, storey=2, field="gallery-door")
b.doorway("hub", "E", 0, width=2)

# Course 2, the Slopes. A portal throws a body out square to its surface,
# so the steep ramp throws it flat into the ledge's face and the shallow one
# up and over onto it. Only an eight-storey fall is fast enough: the loft's
# hatch over the chute, whose floor is shot through its mouth on the pit
# floor. The balcony's fall is too slow for either ramp, and around the pit
# only the lowest two storeys of wall take a portal, so no wall throws a
# body high.
SLOPE = {**dict.fromkeys(("bottom", "north", "south", "east", "west"), "metal"), "top": "pit-floor"}
b.room("slopes", level=0, at=(29, 32), size=(19, 12), storeys=11, **METAL)
b.face_wall(0, (34, 32), "N", "brick", length=11, storeys=2)
b.face_wall(0, (34, 43), "S", "brick", length=11, storeys=2)
b.platform("balcony", level=GROUND, at=(29, 32), size=(5, 12), material="metal")
b.wall(level=0, start=(34, 32), end=(34, 40), material="brick", storeys=2)
b.wall(level=2, start=(34, 32), end=(34, 44), material="metal")
b.ramp("shallow", lower_level=0, at=(36, 33), size=(5, 3), direction="W", material=SLOPE)
b.ramp("steep", lower_level=0, at=(38, 37), size=(2, 3), direction="W", levels=2, material=SLOPE, allow_steep=True)
b.ladder(lower_level=0, landing=(33, 33), side="E", levels=3)
b.platform("ledge2", level=6, at=(45, 32), size=(3, 12), material="metal")
b.wall(level=0, start=(45, 32), end=(45, 44), material="metal", storeys=6)
b.plate(level="ledge2", at=(46, 38), switch="slopes")

b.room(
    "chute", level=0, at=(31, 40), size=(3, 3), storeys=8, floor="floor", inside="metal", outside="metal", ceiling=False
)
b.doorway("chute", "E", 0, width=3, storeys=2)
b.hole(GROUND, (3, 3), (31, 40))
b.room("loft", level=8, at=(29, 37), size=(5, 7), storeys=2, **METAL)
b.hole("loft", (3, 3), (31, 40))
b.room("stair", level=GROUND, at=(29, 35), size=(2, 2), storeys=7, **METAL)
b.doorway("stair", "E", 0, width=2)
b.doorway("stair", "S", 0, width=2, storeys=2, storey=5)
b.ladder(lower_level=GROUND, landing=(30, 37), side="N", levels=5)
b.doorway("hub", "S", 9, width=2)

# The ledge's way back: a door barred until the plate is pressed, and a ramp
# down to a door in the hub's east wall.
b.room("corridor", level=GROUND, at=(32, 28), size=(16, 4), storeys=5, **HUB)
b.ramp("climb", lower_level=GROUND, at=(35, 28), size=(8, 4), direction="E", levels=3, material="hub-floor")
b.platform("top", level=6, at=(43, 28), size=(5, 4), material="hub-floor")
b.doorway("corridor", "S", 13, width=2, storeys=2, storey=3, field="slopes-door")
b.doorway("hub", "E", 8, width=2)

b.room("exit", level=GROUND, at=(24, 32), size=(4, 8), storeys=2, **HUB)
b.doorway("hub", "S", 4, width=4)
for level in (GROUND, GROUND + 1):
    b.barrier(level=level, start=(24, 34), end=(28, 34), field="gallery")
    b.barrier(level=level, start=(24, 36), end=(28, 36), field="slopes")
b.plate(level="exit", at=(26, 38), switch="finish")
b.fireworks("finish")

b.room_lights("hub", "decorative", every=4)
b.room_lights("exit", "decorative", every=4)
b.room_lights("hall", "utility", every=4, storey=2)
b.room_lights("shaft", "utility", every=4, storey=2)
b.room_lights("slopes", "utility", every=4)
b.room_lights("corridor", "decorative", every=4)

b.save()
