# Portal Primer

A single-player course of six chambers, each built around one portal idea:
the fling, preparing a portal from somewhere else, a switch and a key, a
speed run, a low-gravity jump, and an eraser before the final drop. There are
no enemies. The map was authored by an AI with `tools/mapauthor.py`;
`build.py` beside this file is its source and writes `layout.json`.

```sh
cargo run --release -- --map portal_primer --look 90,-10
python3 tools/mapauthor.py build portal_primer
python3 tools/editor.py portal_primer
```

The grid is 2 m with 2.2 m storeys; movement uses the game's defaults. The
portal gun is always available. Pale surfaces take portals, dark ones do not.
Left click places portal A, right click portal B. Yellow chevrons grant speed,
the white arrow low gravity, and the violet curtain erases both. Pickups
reappear at once, so a retry never runs short.

## Hints

1. **The fling.** Put A on the pale pad below the lobby's edge and B on the
   pale wall across the pit. Walk off the edge, release movement over the pad,
   and the fall carries you out of the wall onto checkpoint 1.
2. **Prepare elsewhere.** The corridor's north wall takes portals on its far
   side only. Climb the ladder at the corridor's end onto the gallery, place B
   on that face, come back down, place A on the pad past the corridor's end,
   and walk off into it.
3. **The island.** From checkpoint 2, put B high on the tower across the gap
   and A on the wall beside you, then run into A. On the island, step on the
   cyan plate to power the bridge and take the key, then cross the bridge and
   walk through the gate to checkpoint 3.
4. **The speed run.** Take the chevrons, run up the ramp at full speed, and
   jump at its top. Falling short lands you on the refill deck: take its
   chevrons and climb the ladder back onto the runway.
5. **The summit.** Take the white arrow. Run off the east edge, jump, let go,
   and hold forward again on the way down to land on the summit.
6. **The drop.** The violet doorway on the summit's east side is the only way
   into the tower, and it takes your pickups. From the lip of the ledge inside,
   put A on the big pale pad far below and B on the pale wall to the north.
   Walk off the ledge over the pad: without low gravity the fall is fast, and
   the wall flings you onto the finish. Walk to the gold plate.

## Automated route

```sh
cargo run --release -- --experiment config/server/maps/portal_primer/experiment.json | python3 tools/mapauthor.py proof portal_primer -
cargo run --release -- --play-experiment config/server/maps/portal_primer/experiment.json
cargo test --release -p cuboid-wars primer_tests
```

The script places every portal with ordinary shots and plays the course
through the client's movement. The tests pin the full run (five checkpoints,
four crossings, the plate, the key, both pickups, the erasure, and the
fireworks without a death), and the failures the course is built on: the
first fling without its exit portal, the hidden wall shot from the corridor,
the bridge before the plate, the gate without the key, the ramp gap without
speed and the refill retry, the summit without low gravity, the tower shut to
a player who kept low gravity (past the summit, beside the doorway, over the
wall), and the ledge drop without its portals.
