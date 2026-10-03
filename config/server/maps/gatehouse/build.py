#!/usr/bin/env python3
"""Gatehouse: a hub whose exit three gates bar, and a room that lowers each."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[4] / "tools"))
from map_author.builder import MapBuilder  # noqa: E402

# Brick, tile, and plaster take a portal; metal does not. A storey is 1.6 m:
# a jump clears one and not two, a room is two tall, and a portal fills two.
b = MapBuilder("gatehouse", cols=56, rows=44, levels=9, solid="metal", portal="brick", default="brick")
# The hub stands on level 3, so a room can sink a pit three storeys below it.
GROUND = 3
for level, name in {0: "Pits", GROUND: "Ground", 5: "Ceilings", 6: "Roof", 8: "Hall roof"}.items():
    b.level_name(level, name)

# A room's plate latches: crossing it again on the way out undoes nothing.
b.switch("drop", activation="latch", reset="never", color="#00ccff")
b.switch("cistern", activation="latch", reset="never", color="#ffcc00")
b.switch("firing", activation="latch", reset="solo", color="#ff5533")
b.switch("finish", activation="momentary", reset="never", color="#33dd66")
# The shield stands only while its plate is held.
b.switch("shield", activation="momentary", reset="never", color="#88ddff")
for gate in ("drop", "cistern", "firing"):
    b.field(gate, switch=gate, initially_on=True)
b.field("cistern-gate", switch="cistern", initially_on=True, color="#ffcc00")
b.field("shield", switch="shield", initially_on=False, color="#88ddff")

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

# Room 2, the Cistern. A hall five storeys tall with an open metal tank three
# storeys tall standing two cells off its east wall, the plate at the bottom
# behind a tall gate that shows the inside and, looking up through it, the
# hall's ceiling where a roof would be. The tank is metal inside and out, and
# so is the ceiling over it and the gap beside it, so nothing opens inside and
# a fall from above lands beside it. The way in is a wall portal above the
# rim: a body dropping into a floor portal leaves it a storey over the tank
# with the speed of its fall, enough to carry over the rim and down inside,
# while a portal below the rim throws it at the tank's side. The tank's plate
# opens the gate and the hub's.
b.room("hall", level=GROUND, at=(22, 4), size=(16, 16), storeys=5, floor="floor", inside="brick", ceiling="ceiling")
METAL = dict(floor="metal", inside="metal", outside="metal")
b.room("tank", level=GROUND, at=(30, 9), size=(6, 5), storeys=3, ceiling=False, **METAL)
b.face_slab(GROUND + 5, (9, 7), (29, 8), "metal", face="bottom")
b.plate(level="tank", at=(32, 11), switch="cistern")
b.doorway("tank", "S", 2, width=2, storeys=2, field="cistern-gate")

# The hub: the start, a door to each room, and the corridor out.
HUB = dict(floor="hub-floor", inside="hub-wall", outside="outside", ceiling="ceiling")
b.room("hub", level=GROUND, at=(24, 20), size=(12, 12), storeys=2, **HUB)
b.checkpoint(0, level="hub", at=(29, 25), size=(2, 2))
b.doorway("hub", "W", 5, width=2)
b.doorway("hub", "N", 5, width=2)

# Room 3, the Firing Line. A range four storeys tall with a turret at the
# front edge of a two-storey metal plinth across its far end, in sight of
# everything on the floor but the pen under its own feet: a metal box in the
# plinth's face, open to the range, the plate at its back and the wall behind
# it brick. The plate on the threshold raises a field along the plinth's edge
# that stops the beam while it is held, long enough to aim at the pen's back
# wall. The other portal goes on the foyer's north wall, which the turret
# cannot see, and the pair carries a body in and back unseen. Walking the
# range is more than a second of fire, more than a body has. In co-op one
# player holds the shield while the other walks. The foyer's doors are offset
# so no line from the turret reaches the hub. The pen's plate opens the hub's
# gate and resets when its presser dies.
b.room("foyer", level=GROUND, at=(36, 24), size=(2, 6), storeys=2, **HUB)
b.doorway("foyer", "W", 1, width=2)
b.doorway("foyer", "E", 3, width=3)
b.room("range", level=GROUND, at=(38, 21), size=(17, 10), storeys=4, floor="floor", inside="brick", ceiling="ceiling")
b.doorway("range", "W", 6, width=3)
b.platform("plinth", level=GROUND + 2, at=(52, 21), size=(3, 10), material="metal")
for start, end in (((52, 21), (52, 25)), ((52, 27), (52, 31)), ((52, 25), (55, 25)), ((52, 27), (55, 27))):
    b.wall(level=GROUND, start=start, end=end, material="metal", storeys=2)
b.face_slab(GROUND, (3, 2), (52, 25), "metal")
b.actor_zone("turret", level="plinth", size=(1, 2), at=(52, 25))
for level in (GROUND + 2, GROUND + 3):
    b.barrier(level=level, start=(52, 21), end=(52, 31), field="shield")
b.plate(level="range", at=(38, 28), switch="shield")
b.plate(level="range", at=(53, 25), switch="firing")

b.room("exit", level=GROUND, at=(28, 32), size=(4, 10), storeys=2, **HUB)
b.doorway("hub", "S", 4, width=4)
for row, gate in ((34, "drop"), (36, "cistern"), (38, "firing")):
    for level in (GROUND, GROUND + 1):
        b.barrier(level=level, start=(28, row), end=(32, row), field=gate)
b.plate(level="exit", at=(30, 40), switch="finish")
b.fireworks("finish")

# One row of lights where people walk, at the height a single wall would
# hang them, and the pit's own on the faces under the landing and the block.
b.room_lights("hub", "decorative", every=4)
b.room_lights("exit", "decorative", every=4)
b.room_lights("drop", "utility", every=4, storey=GROUND)
for at, side in (((20, 24), "E"), ((20, 27), "E"), ((10, 26), "W")):
    b.light(level=0, at=at, side=side, kind="utility", height=1.9)
# The hall's east wall is lit clear of the tank, and the tank from inside.
b.room_lights("hall", "decorative", every=4, sides="NSW")
for row in (6, 16):
    b.light(level=GROUND, at=(37, row), side="E", kind="decorative", height=1.9)
for at, side in (((30, 11), "W"), ((35, 11), "E")):
    b.light(level=GROUND, at=at, side=side, kind="utility", height=2.4)
b.room_lights("range", "utility", every=4, sides="NS")
b.light(level=GROUND, at=(53, 25), side="N", kind="utility", height=1.9)

b.save()
