#!/usr/bin/env python3
"""Gatehouse: a hub whose exit three gates bar, and a room that lowers each."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[4] / "tools"))
from map_author.builder import MapBuilder  # noqa: E402

# Brick, tile, and plaster take a portal; metal does not. A storey is 1.6 m:
# a jump clears one and not two, a room is two tall, and a portal fills two.
b = MapBuilder("gatehouse", cols=56, rows=44, levels=7, solid="metal", portal="brick", default="brick")
# The hub stands on level 3, so a room can sink a pit three storeys below it.
GROUND = 3
for level, name in {0: "Pits", GROUND: "Ground", 5: "Ceilings", 6: "Roof"}.items():
    b.level_name(level, name)

b.switch("drop", activation="toggle", reset="never", color="#00ccff")
b.switch("vantage", activation="toggle", reset="never", color="#ffcc00")
b.switch("firing", activation="toggle", reset="never", color="#ff5533")
b.switch("finish", activation="momentary", reset="never", color="#33dd66")
for gate in ("drop", "vantage", "firing"):
    b.field(gate, switch=gate, initially_on=True)

# Room 1, the Drop. The landing by the door looks down a pit at a metal block
# with the plate on top, two storeys above the pit's floor: too high to jump
# onto, too far to jump to, and clad in metal with the walls and ceiling
# around it, so no portal opens within a walking body's reach of it. A fall
# from the landing into one portal in the pit's floor leaves another beside
# the block fast enough to rise past its top and steer onto it.
b.room(
    "drop", level=0, at=(5, 22), size=(19, 8), storeys=6, floor="pit-floor", inside={"W": "metal"}, ceiling="ceiling"
)
b.face_wall(0, (5, 22), "N", "metal", length=13, storeys=6)
b.face_wall(0, (5, 29), "S", "metal", length=13, storeys=6)
b.face_slab(6, (13, 8), (5, 22), "metal", face="bottom")
b.platform("landing", level=GROUND, at=(21, 22), size=(3, 8), material="floor")
b.wall(level=0, start=(21, 22), end=(21, 30), storeys=3)
b.ladder(lower_level=0, landing=(21, 29), side="W", levels=3)
b.platform("block", level=2, at=(5, 24), size=(5, 4), material="metal")
for start, end in (((5, 24), (10, 24)), ((5, 28), (10, 28)), ((10, 24), (10, 28))):
    b.wall(level=0, start=start, end=end, material="metal", storeys=2)
b.plate(level="block", at=(7, 26), switch="drop")

# The hub: the start, a door to each room, and the corridor out.
HUB = dict(floor="hub-floor", inside="hub-wall", outside="outside", ceiling="ceiling")
b.room("hub", level=GROUND, at=(24, 20), size=(12, 12), storeys=2, **HUB)
b.checkpoint(0, level="hub", at=(29, 25), size=(2, 2))
b.doorway("hub", "W", 5, width=2)

b.room("exit", level=GROUND, at=(28, 32), size=(4, 10), storeys=2, **HUB)
b.doorway("hub", "S", 4, width=4)
for row, gate in ((34, "drop"), (36, "vantage"), (38, "firing")):
    for level in (GROUND, GROUND + 1):
        b.barrier(level=level, start=(28, row), end=(32, row), field=gate)
b.plate(level="exit", at=(30, 40), switch="finish")
b.fireworks("finish")

# One row of lights a little above head height where people walk, and the
# pit's own on the faces under the landing and the block.
b.room_lights("hub", "decorative", every=4)
b.room_lights("exit", "decorative", every=4)
b.room_lights("drop", "utility", every=4, storey=GROUND + 1)
for at, side in (((20, 24), "E"), ((20, 27), "E"), ((10, 26), "W")):
    b.light(level=1, at=at, side=side, kind="utility")

b.save()
