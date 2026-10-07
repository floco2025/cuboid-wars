#!/usr/bin/env python3
"""Switchyard: leave a door behind, change the building, then come back through it."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[4] / "tools"))
from map_author.builder import MapBuilder  # noqa: E402

b = MapBuilder("switchyard", cols=48, rows=48, levels=11, solid="metal", portal="brick", default="brick")
for level, name in {
    0: "Undercroft",
    2: "Concourse",
    4: "Roofs and catch deck",
    6: "Signal balcony",
    8: "Sliding canopy",
}.items():
    b.level_name(level, name)

for name, color in (("dispatch", "#ff8833"), ("canopy", "#aa77ff")):
    b.switch(name, activation="toggle", reset="never", color=color)
for name, color in (("freight-return", "#00ccff"), ("undercroft-return", "#ddddee")):
    b.switch(name, activation="latch", reset="never", color=color)
b.switch("finish", activation="latch", reset="never", color="#ffffff")
b.field("loading-door", switch="dispatch", initially_on=False, color="#ff8833")
b.field("freight-return", switch="freight-return", color="#00ccff")
b.field("undercroft-return", switch="undercroft-return", color="#ddddee")
b.field("return-bridge", switch="undercroft-return", initially_on=False, color="#ddddee")
b.field("cyan", color="#00ccff")
b.field("green", color="#44dd88")
b.field("amber", color="#ffcc33")
b.field("rail-window", color="#88aabb")

PUBLIC = dict(floor="tile", inside="wall", outside="outside", ceiling="ceiling")
WORK = dict(floor="floor", inside="brick", outside="outside", ceiling="ceiling")
METAL = dict(floor="metal", inside="metal", outside="metal", ceiling="metal")

# A sealed rail gallery: its carriage is the only way past the departure
# interlock. The controller is far from its door, so a solo player leaves a
# portal inside before dispatching it. The destination opens a lasting return.
b.room("rail", level=0, at=(4, 4), size=(40, 6), storeys=5, **METAL)
b.room("departure", level=2, at=(4, 10), size=(24, 10), storeys=2, **WORK)
b.room("arrival", level=2, at=(28, 10), size=(16, 10), storeys=2, **PUBLIC)
b.doorway("departure", "N", 5, width=4, field="loading-door")
b.doorway("departure", "N", 14, width=8, field="rail-window")
b.doorway("arrival", "N", 7, width=4)
b.doorway("departure", "E", 4, width=3, field="freight-return")
b.plate(2, (20, 15), switch="dispatch")
b.plate(2, (36, 15), switch="freight-return")
b.item("key", 2, (38, 15), field="cyan")
b.room("approach", level=2, at=(20, 20), size=(4, 4), storeys=2, **PUBLIC)
b.doorway("departure", "S", 16, width=4)
b.doorway("approach", "N", 0, width=4)
b.doorway("approach", "S", 0, width=4)

car = b.geometry("freight-car", cols=6, rows=5, levels=3)
car.room(
    "cabin",
    level=0,
    at=(0, 0),
    size=(6, 5),
    storeys=2,
    floor="metal",
    inside="brick",
    outside="metal",
    ceiling="ceiling",
)
car.face_slab(2, (6, 5), (0, 0), "metal")
car.doorway("cabin", "S", 1, width=4)
car.room_lights("cabin", "utility", every=4, sides="EW", height=2.4)
b.nested_map(
    "freight",
    geometry="freight-car",
    level=2,
    at=(8, 5),
    to=(34, 5),
    travel_secs=5.0,
    motion="follow_switch",
    switch="dispatch",
    initially_on=False,
    from_nudge=(0, 0, -1.05),
    to_nudge=(0, 0, -1.05),
)

b.room("concourse", level=2, at=(20, 24), size=(12, 12), storeys=2, **PUBLIC)
b.checkpoint(0, level=2, at=(25, 30), size=(2, 2))
b.doorway("concourse", "N", 0, width=4)
b.doorway("concourse", "W", 4, width=3)
b.doorway("concourse", "E", 4, width=4)

# The green key opens the archive vault but also takes away the bridge.
# Its doorway faces away from the entrance: only a player out on the lit
# floor can see a portal surface inside. Prepare that door before the key
# removes the footing. The lower room catches mistakes and has a ladder out.
b.room("archive", level=0, at=(4, 24), size=(16, 20), storeys=4, **METAL)
b.face_wall(2, (19, 24), "E", "brick", length=20, storeys=2)
b.platform("archive-entry", level=2, at=(18, 24), size=(2, 20), material="tile")
b.bridge("glass-floor", level=2, at=(8, 28), size=(10, 16), field="green")
b.bridge("north-floor", level=2, at=(8, 24), size=(10, 1), field="green")
b.bridge("vault-approach", level=2, at=(4, 36), size=(4, 8), field="green")
b.room(
    "vault",
    level=2,
    at=(4, 24),
    size=(4, 12),
    storeys=2,
    floor="metal",
    inside="brick",
    outside="metal",
    ceiling="metal",
)
b.doorway("vault", "S", 1, width=3)
for level in (2, 3):
    b.barrier(level, (4, 29), (8, 29), field="green")
b.wall(2, (10, 36), (10, 42), material="metal", storeys=2)
b.platform("inspection-perch", level=1, at=(9, 42), size=(2, 2), material="metal")
b.ramp("inspection-ramp", lower_level=0, at=(11, 42), size=(3, 2), direction="W", material="metal")
b.item("key", 2, (19, 33), field="green")
b.item("key", 2, (6, 26), field="amber")
b.plate(2, (6, 27), switch="undercroft-return")
b.wall(0, (18, 24), (18, 40), material="metal", storeys=2)
b.wall(0, (18, 43), (18, 44), material="metal", storeys=2)
b.wall(0, (8, 24), (8, 36), material="metal", storeys=2)
b.ladder(0, (18, 41), "W", levels=2)
b.doorway("archive", "E", 4, width=3, storey=2)
# The lower return stays available after a misplaced shot or an early key.
# Completing the vault opens a direct upper shortcut as well.
b.doorway("vault", "E", 1, width=3, field="undercroft-return")
b.bridge("vault-return", level=2, at=(8, 25), size=(10, 3), field="return-bridge")

# Two independently earned keys grant access to the last machine.
b.room("ticket-gates", level=2, at=(32, 28), size=(6, 4), storeys=2, **METAL)
b.doorway("ticket-gates", "W", 0, width=4)
b.doorway("ticket-gates", "E", 0, width=4)
for level in (2, 3):
    b.barrier(level, (34, 28), (34, 32), field="cyan")
    b.barrier(level, (36, 28), (36, 32), field="amber")

# A portalable ceiling travels above a high balcony. A door left underneath
# it becomes a way onto that balcony after dispatch; an unprepared journey
# drops to the broad catch deck instead. Both landings are below safe fall height.
b.room("signal-room", level=2, at=(38, 24), size=(8, 12), storeys=8, **METAL)
b.doorway("signal-room", "W", 4, width=4)
for at, side, length in (((38, 24), "N", 8), ((45, 24), "E", 12), ((38, 35), "S", 8)):
    b.face_wall(2, at, side, "brick", length=length, storeys=2)
b.platform("catch-deck", level=4, at=(38, 30), size=(8, 6), material="metal")
b.ramp("catch-ramp", lower_level=2, at=(38, 31), size=(3, 4), direction="N", levels=2, material="metal")
b.platform("signal-balcony", level=6, at=(38, 24), size=(8, 4), material="metal")
for level in range(6, 10):
    b.barrier(level, (38, 28), (46, 28), field="rail-window")
b.plate(2, (41, 26), switch="canopy")
b.checkpoint(1, level=2, at=(42, 28), size=(2, 2))
b.platform("signal-terrace", level=6, at=(32, 20), size=(14, 4), material="tile")
b.doorway("signal-room", "N", 2, width=4, storey=4)
b.plate(6, (40, 22), switch="finish")
b.fireworks("finish")
canopy = b.geometry("sliding-canopy", cols=6, rows=6, levels=1)
canopy.platform("roof", level=0, at=(0, 0), size=(6, 6), material="metal")
canopy.face_slab(0, (6, 6), (0, 0), "ceiling", face="bottom")
b.nested_map(
    "canopy",
    geometry="sliding-canopy",
    level=8,
    at=(39, 30),
    to=(39, 24),
    travel_secs=4.0,
    motion="follow_switch",
    switch="canopy",
    initially_on=False,
)

for name in ("concourse", "approach", "arrival"):
    b.room_lights(name, "decorative", every=4)
b.room_lights("departure", "utility", every=5, sides="ESW")
b.room_lights("archive", "utility", every=5, storey=2, height=2.3, sides="ES")
b.room_lights("vault", "utility", every=5, sides="W", height=2.3)
for row in (27, 33, 39):
    b.light(0, (17, row), "E", kind="utility", height=2.2)
b.room_lights("ticket-gates", "utility", every=4, sides="NS")
b.room_lights("signal-room", "utility", every=4, height=2.3)
b.light(6, (45, 25), "E", kind="utility", height=2.3)
warnings = b.save(quiet=True)
if warnings:
    raise ValueError("\n".join(warnings))
