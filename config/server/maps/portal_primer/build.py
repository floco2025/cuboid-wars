#!/usr/bin/env python3
"""Portal Primer: six chambers, one portal idea each, ending in fireworks."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[4] / "tools"))
from map_author.builder import MapBuilder  # noqa: E402

b = MapBuilder("portal_primer", cols=80, rows=32, levels=12)
LEVEL_NAMES = {0: "Pit", 1: "Refill", 2: "Island", 3: "Runway top", 4: "Lobby", 5: "Gallery", 7: "Summit", 8: "Roof"}
for level, name in LEVEL_NAMES.items():
    b.level_name(level, name)
b.switch("island", activation="toggle", reset="never", color="#00ccff")
b.switch("finish", activation="momentary", reset="never", color="#ffcc00")
b.field("span", switch="island", initially_on=False, color="#00ccff")
b.field("gate", initially_on=True, color="#ff5533")

# Chamber 0, the fling: drop off the lobby into the pad below, fly out of the
# wall across the pit and land on the far platform.
b.platform("lobby", level=4, at=(2, 24), size=(4, 4))
b.checkpoint(0, level="lobby", at=(3, 25), size=(2, 2))
b.portal_floor("pad0", level="lobby", down=2, east_of="lobby", gap=1, shift=1)
b.portal_wall("wall0", level=6, at=(10, 29), side="S", length=2)
b.platform("landing1", level=4, at=(9, 20), size=(5, 6))
b.checkpoint(1, level="landing1", at=(10, 22), size=(2, 2))

# Chamber 1, preparing elsewhere: the corridor's north wall takes a portal on
# its far side only, seen from the gallery up the ladder; the pad waits past
# the corridor's end.
b.platform("corridor1", level=4, size=(12, 2), east_of="landing1", gap=0, shift=2)
b.platform("nook", level=4, at=(25, 21), size=(1, 1))
b.wall(level=4, start=(26, 22), end=(26, 23))
b.platform("gallery1", level=5, at=(23, 16), size=(4, 5))
b.ladder(lower_level=4, landing=(25, 20), side="S")
b.portal_wall("wall1", level=4, at=(19, 21), side="S", length=2)
b.portal_floor("pad1", level="corridor1", down=2, east_of="corridor1", gap=1)
b.platform("landing2", level=2, at=(17, 13), size=(6, 6))
b.checkpoint(2, level="landing2", at=(19, 15), size=(2, 2))

# Chamber 2, the island: run north into the landing's wall, fly west out of
# the tower onto the island. The plate powers the bridge onward and the key
# opens the gate at its end.
b.portal_wall("wall2a", level="landing2", at=(19, 13), side="N")
b.platform("island", level=2, size=(5, 5), east_of="landing2", gap=5, shift=1)
b.wall(level=2, start=(33, 14), end=(33, 19), storeys=2)
b.portal_wall("wall2b", level=4, at=(32, 16), side="E")
b.plate(level="island", at=(29, 15), switch="island")
b.item("key", level="island", at=(31, 18), field="gate")
b.bridge("bridge2", level="island", size=(2, 4), field="span", north_of="island", gap=0, shift=2)
b.barrier(level=2, start=(30, 10), end=(32, 10), field="gate")
b.platform("deck3", level=2, at=(28, 6), size=(6, 4))
b.checkpoint(3, level="deck3", at=(30, 7), size=(2, 2))

# Chamber 3, the speed run: the ramp and the gap after it need the pickup;
# a short jump lands on the refill deck with a ladder back up.
b.platform("runway", level=2, size=(8, 2), east_of="deck3", gap=0, shift=1)
b.item("speed", level="runway", at=(36, 8))
b.ramp("ramp3", lower_level=2, size=(3, 2), direction="E", east_of="runway", gap=0)
b.platform("landing4", level=3, size=(5, 4), east_of="ramp3", gap=5, shift=-1)
b.checkpoint(4, level="landing4", at=(51, 7), size=(2, 2))
b.platform("refill", level=1, at=(40, 6), size=(16, 5))
b.ladder(lower_level=1, landing=(41, 8), side="S")
b.item("speed", level="refill", at=(48, 8))

# Chamber 4, the summit: four storeys up across a chasm, which only a
# low-gravity jump rises to; holding forward on the way down lands it.
b.item("low_gravity", level="landing4", at=(53, 8))
b.platform("summit", level=7, size=(7, 6), east_of="landing4", gap=5, shift=-1)
b.checkpoint(5, level="summit", at=(60, 5), size=(6, 6))

# Chamber 5, the finale. Low gravity floats down to any floor it can reach, so
# the drop and the finish stand in a closed, roofed tower whose one way in is
# the eraser doorway on the summit's lip. Past it the fall from the ledge is
# at full gravity and lethal; the pad catches it and the wall flings the body
# south onto the finish.
WEST, NORTH, EAST, SOUTH = 67, 3, 73, 24
DOOR_ROW = 8
b.portal_floor("pad5a", level=0, at=(69, 7), size=(4, 3))
b.portal_wall("wall5", level=2, at=(69, NORTH), side="N", length=2)
b.platform("finish", level=0, at=(WEST, 10), size=(6, 14))
b.plate(level="finish", at=(70, 22), switch="finish")
b.platform("ledge", level="summit", at=(WEST, 7), size=(2, 3))
b.platform("roof", level="summit", up=1, at=(WEST, NORTH), size=(EAST - WEST, SOUTH - NORTH))
for start, end in (
    ((WEST, NORTH), (EAST, NORTH)),
    ((EAST, NORTH), (EAST, SOUTH)),
    ((WEST, SOUTH), (EAST, SOUTH)),
):
    b.wall(level=0, start=start, end=end, storeys=8)
b.wall(level=0, start=(WEST, NORTH), end=(WEST, SOUTH), storeys=7)
b.wall(level="summit", start=(WEST, NORTH), end=(WEST, DOOR_ROW))
b.wall(level="summit", start=(WEST, DOOR_ROW + 1), end=(WEST, SOUTH))
b.eraser(level="summit", start=(WEST, DOOR_ROW), end=(WEST, DOOR_ROW + 1))
b.fireworks("finish")

b.save()
