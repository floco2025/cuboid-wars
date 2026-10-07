# Map authoring for AIs

`tools/mapauthor.py` is how an AI composes a map, looks at it, measures it,
and proves a route, without a renderer. Every rule and number comes from the
game: the builder validates with `map_core`, flights run the game's own
step, funnel, and portal hop, and shots, sweeps, and the proof run in the
game itself through the headless experiment runner. An AI-authored map keeps its `build.py` beside its `layout.json`;
that script is the map's source, and a human who edits the layout in the
editor updates or deletes it.

The first command builds the Rust map library (`cargo build --release -p map_core_py`), which takes a few minutes once. `shots` and `sweep` run the game with `cargo run --release`, which builds it whenever it is stale.

```sh
python3 tools/mapauthor.py build <map>                 # run config/server/maps/<map>/build.py
python3 tools/mapauthor.py describe <map> [--level N] [--plan | --summary]
python3 tools/mapauthor.py where <map> L4:9.5,17.5 | --world -46,8.8,-30
python3 tools/mapauthor.py surface <map> wall:L4:6,16:W[:along] | floor:L2:37,42
python3 tools/mapauthor.py jump <map> --from L4:11,17:E [--walk] [--late 0.1] [--air-control]
python3 tools/mapauthor.py fling <map> --from L4:6,17:W --into --entry wall:L4:6,17:W --exit wall:L6:20,23:S
python3 tools/mapauthor.py ranges <map>
python3 tools/mapauthor.py shots <map> --from L1:4,6 [surface ...]
python3 tools/mapauthor.py sweep <map> --from L1:4,6 --moves "move 0,-1 x40; advance 20" [--goal L1:2,2:6,4]
python3 tools/mapauthor.py sweep <map> --from L1:4,6 --walk-in --goal L1:2,2:6,4
python3 tools/mapauthor.py sweep <map> --after <route.json>:52 --from L1:4,6 ...   # shots, sweep, or walk-in with a route's state
cargo run --release -- --experiment <map> | python3 tools/mapauthor.py proof <map> -
```

## Workflow

1. Register the map in `config/server/gameplay.json::maps` and write its `settings.json`: `textures` with an alias per role, a floor, wall, and ceiling material from Hotel's beside the `solid` and `portal` pair, and nothing else the map's idea does not need; whatever overrides a default comes first in the file, in `gameplay.json`'s order, the map's own content after it. `gameplay.json`'s defaults are the one game every map plays: 1 m cells and 1.6 m storeys, the shared movement, pickups that last until death or erasure, and the portal gun always held. The tools never write either file.
2. Write `build.py` (below), run `build`, read the summary it prints.
3. `describe` for the plan of each level; `surface`, `jump`, `fling`, and `ranges` for the physics of each gap and portal pair. Place landings where a flight comes down, not where it looks right.
4. `shots` from each point a portal is shot from, `sweep` with the moves of each puzzle step, and `sweep --walk-in` for each goal (Asking the game): the intended pair reaches the goal, and walking into a pair never does.
5. Write `experiment.json` beside the layout (format under Proving a route), run it, read `proof`. Every `aim`, `check`, and `spawn` is in world metres; `describe` and `where` give them.
6. Iterate until the route passes, then pin it in `src/experiment/tests/` like `foundry.rs`, on `scenario`, which pins the shared movement numbers the course was proved against: the completion run and the failures the course is built on (a missing portal, a skipped pickup, a plate not pressed). A route the runner cannot finish because the player died ends the run at the next `aim`; truncate the script to see the report up to there.
7. `python3 tools/editor.py <map>` opens the result; Check Map must be clean.

## Coordinates

Cells and levels everywhere in the tools: `(col, row)` with columns along x and rows along z, rects end-exclusive, level L's floor top at `L * level_height`. The grid is centred on the world origin, so world x = `col * cell - cols * cell / 2`, the same for z, and feet y = `L * level_height`. Scripts take world metres; `where` converts both ways. A takeoff `L4:11,17:E` leaves cell (11, 17) on level 4 over its east edge; a surface `wall:L4:6,17:W` is the wall on the west side of cell (6, 17) with its portal facing that cell, and `floor:L2:37,42` a floor point at cell coordinates (37, 42), which is the corner shared by cells 36 and 37. A takeoff and a wall take an optional `:<along>` from 0 at the edge's west or north end to 1 at the other, the middle without one: `wall:L1:6,2:N:1` centres a portal on the point cells 6 and 7 share, where a portal two cells wide belongs.

## Rules of thumb at the shared defaults

Cell 1.0 m, level 1.6 m, floors and walls 0.2 m thick. Move 5.1 m/s, jump 12 m/s (apex 2.88 m, so a jump clears one storey and not two), gravity 25, low gravity 13.2 (apex 5.45 m: three storeys and not four), the speed pickup ×1.818 (9.3 m/s), air steering 3 m/s² and no air braking, so a flight keeps its speed. A running jump carries about 4.9 m, 8.9 m with speed. Fall damage starts at an 8 m equivalent drop and is lethal at 15 m; a light body lands like one that fell about half as far. The body is 0.6 m wide and 1.8 m tall, eyes at 1.62 m. The funnel pulls a falling body onto a floor portal from 0.6 m plus 0.8 m per second still to fall, and never changes its velocity, so a fling leaves at the angle it entered.

Low gravity carries a jump three storeys up and a run about twice as far, so a goal it must not reach stands four storeys above every floor near it or in a room entered through an `eraser` edge; eraser pickups can be walked around. A pickup reaches its player through the server, so a gravity change in flight arrives a tick or two late: leave metres of margin on a flight that catches one.

Do not trust these for a gap: run `ranges` once per map and `jump` at the actual edge. A player crossing a gap holds forward, which is the `held` row and the `--air-control` lines.

## Portal backing

A portal is 1.4 × 2.6 m and its rim, 6% larger, needs backing: 1.5 × 2.8 m of portalable surface. How many cells and wall sections that is follows from the map's geometry, and the summary's first lines and `BuildError`s say it. At the Primer's 2 m cells and 2.2 m levels a wall portal is one cell wide and two stacked sections tall, and a floor portal two cells long; at 1 m cells a wall portal is two cells wide and a floor portal three by two; at Hotel's 4.4 m levels one wall section is enough. `portal_wall` and `portal_floor` write what fits by default. A wall portal also needs nothing standing in front of its upper sections, so the room it is in is as tall as the portal; a floor portal needs no wall inside its block and no pressure plate within 1.2 m; a wall light keeps a portal 0.4 m away. A surface's normal fixes the exit direction; where the portal is shot from only turns a floor portal by quarter turns. Materials come from the map's `settings.json::textures` aliases and their `portalable` flag. `surface` and the summary's "portal-ready" list judge all of this by grid rules; the runner's `portal` action is the final word.

## The builder

```python
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[4] / "tools"))
from map_author.builder import MapBuilder

b = MapBuilder("portal_ascent", cols=48, rows=40, levels=12, solid="solid", portal="portal")
b.level_name(4, "Lobby")
lobby = b.platform("lobby", level=4, at=(4, 18), size=(4, 4))
b.checkpoint(0, level="lobby", size=(2, 2), at=(5, 19))
b.portal_floor("pad1", level="lobby", down=2, east_of="lobby", gap=1)
b.portal_wall("fling1", level=4, at=(20, 19), side="E")
b.platform("landing", level=4, size=(4, 4), east_of="lobby", gap=6, shift=0)
b.save()
```

Placement is `at=(col, row)` or exactly one of `east_of`, `west_of`, `north_of`, `south_of` naming an earlier piece, with `gap` cells between and `shift` cells along the shared edge; `level` is a number or a piece's name, with `up` and `down`. Pieces: `platform`, `hole(level, size, at)`, which takes the floor slabs out of a footprint for a hatch or a shaft, `portal_floor(name, level, size=None)`, which also turns a floor already there into a pad, `portal_wall(name, level, at, side, length=None)`, which makes the face into the cell portalable and keeps the other faces of a wall already there, `wall(level, start, end, material=None, storeys=1)` between grid points, `bridge(..., field=)`, `barrier(level, start, end, field=)`, `eraser(level, start, end)`, `ramp(name, lower_level, size, direction=, levels=1, shape="solid", material=None, allow_steep=False)` where `direction` is the side it rises toward, `material` is an alias or a dict per face whose `top` is the slope at any steepness, and a slope too steep to climb needs `allow_steep`, `ladder(lower_level, landing, side, levels=1)` on a side of its top landing cell, `checkpoint(number, level, size, ...)`, `plate(level, at, switch=)`, `item(type, level, at, field=None)`, `switch(id, activation=, reset=, held=, color=)`, `field(id, switch=, initially_on=, color=)`, `fireworks(switch)`. `save()` normalizes, validates, raises `BuildError` with the validator's messages, writes the layout, and prints the summary. Out of scope: terrain, grounds, random items, quests.

A building is rooms:

```python
b.room("hall", level=1, at=(2, 2), size=(10, 8), storeys=2, floor="carpet", inside="wallpaper", ceiling="plaster")
b.room("vault", level=1, size=(4, 4), east_of="hall", storeys=2, inside={"E": "portal"})
b.doorway("hall", "E", 1)  # into the vault, one storey tall
b.doorway("hall", "S", width=2, eraser=True)  # centred; or field="gate" for a barrier
b.portal_wall("north", level="hall", at=(6, 2), side="N")  # a panel of the hall's north wall
b.portal_floor("pad", level="hall", at=(8, 5))
b.room_lights("hall", "utility", every=3)
b.actor_zone("turret", level="vault", at=(14, 4), size=(1, 1), count=[1, 2], switch="alarm")
```

`MapBuilder(..., solid=, portal=, default=)` names the alias that takes no portal, the one that does, and the one a surface gets when its piece names none: `solid` unless told, and `portal` in a map where portals go almost anywhere. `room` writes the floor, the walls around it `storeys` tall, and the ceiling, which is the floor slab of the level above: `ceiling=False` leaves the top open, and a room built on top gives that slab its own floor material. `inside` and `outside` are the wall faces' aliases, one for all four walls or a dict per side; a wall's ends and edges take its inside, and at `save` a slab's side takes the face of the wall it lies in. Rooms may share a wall, and each keeps its own inside face. `face_wall(level, at, side, material, length=1, storeys=1)` and `face_slab(level, size, at, material, face="top")` change one face of what stands there, which is how a metal end of a room is made. `doorway(room, side, offset=None, width=1, storeys=None, eraser=False, field=None)` counts `offset` in cells from the wall's west or north end and opens as many storeys as a standing body needs. `light(level, at, side, kind=, height=)` hangs one light in a cell on its `side` wall, `height` metres above that level's floor; `room_lights(room, kind, every=3, storey=0, height=None, portal_faces=True)` hangs one row where walls stand, above the floor of its storey `storey`, part way up a single wall as tall as a room unless told, and returns how many it placed. `actor_zone(kind, level, size, ..., count=1, respawn_secs=None, beam_in_secs=0.0, roam=0.0, levels=1, switch=, initially_on=, until_checkpoint=, on_checkpoint=)` never refills a killed actor unless `respawn_secs` says so.

### Moving geometry

`geometry(name, cols=, rows=, levels=)` returns another builder using the
root map's textures and settings. Author its rooms, floors, items, and plates
in local coordinates; the root owns all switches, fields, fireworks, and
geometry definitions. Definitions remain editable until the root is saved.

```python
car = b.geometry("car", cols=6, rows=5, levels=3)
car.room("cabin", level=0, at=(0, 0), size=(6, 5), storeys=2)
car.doorway("cabin", "S", width=3)
b.switch("dispatch", reset="never")
b.nested_map(
    "freight",
    geometry="car",
    level=2,
    at=(8, 5),
    to=(34, 5),
    travel_secs=5,
    motion="follow_switch",
    switch="dispatch",
    initially_on=False,
)
```

`nested_map` places the definition's cell `(0, 0)` at `at` and moves it to
`to`, defaulting to a stationary placement. `to_level` defaults to `level`;
`from_nudge` and `to_nudge` are triples in wall widths, floor thicknesses,
and wall widths. Cycle motion additionally accepts `pause_secs` and
`phase_secs`. Both endpoint footprints must fit the root grid. The root's
native validation resolves references and checks the resulting document.

The summary lists placements, endpoints, motion, switches, and nudges.
Its surface list and the automatic `shots`/`sweep` samples still describe
root geometry. Use explicit world-space `aim` or `probe` targets for carried
surfaces. Experiments run carriers, their switches, rides, and portal
crossings through the shared game simulation. Report states include held
key names and each carrier's current and previous world positions.

## Reading the views

The plan draws one character per cell with the walls between them; the legend is printed with it. The summary names each platform `L<level>.<letter>` with its grid and world bounds and what it holds, the nearest neighbour in each direction with the gap in cells and metres and the storey difference, every portal-ready surface with its world centre and normal (a wall run of several cells also with the spec of its centre, a strip taller than a portal with the highest level a portal can rest on), the portalable surfaces that are not ready and why, ramps with their slope, ladders, barriers, erasers, lights per level, actor zones, switches with their plates, and fields.

`jump` and `fling` print, per pickup scenario, where the flight crosses each floor height, in cells and world metres, with the fall damage and whether a floor is there; the first floor is the landing. With `--air-control` they add how far holding a direction carries on each level. `fling --into` runs into the entry wall from the takeoff cell instead of jumping off an edge; the takeoff and the entry name the same cell, side, and `along`.

`proof` prints one line per action with its result and the events that matter (crossings, landings, checkpoints, pickups, erasure, deaths, portal results), the body's end position as world metres and a cell, and a footer with the checks passed. A passing script shows a route exists; it does not show that the route is necessary or readable, which the design principles in `PLAN.md` cover.

## Asking the game

The summary's portal-ready list and `surface` are grid rules, and `jump` and `fling` fly open air: walls and ceilings stop no flight there. `shots` and `sweep` run the saved map in the game, so its placement rule, its nudge, and its collisions answer.

`shots --from L1:4,6` stands a body on level 1 at that grid point and shoots from its eye at every surface whose material takes a portal: walls on every storey at the height a portal would rest on that storey's floor, floors, ceilings (`ceiling:L<level>:<x>,<z>`, the underside of that level's slab), and ramps (`ramp:L<lower>:<x>,<z>`), one target per portal that fits side by side. It prints one line per stretch of wall and per slab a portal opens on, with how many of its samples open and the first and last of them, and counts the rest as out of sight, no fit, or fizzling. Name surfaces to get the game's verdict on each: where the portal opens and how far the placement rule nudged it, `fizzles`, `no fit`, or `BLOCKED` with what the shot met first. A pad seen only from a ledge's lip shows up here as blocked from where a player would stand. The samples are a few metres apart, so the best spot beside a ledge is a surface to name.

`sweep --from L1:4,6 --moves "move 0,-1 x40 jump; advance 20"` takes the portals `shots` opens from that point (and from every `--also-from` point, for a portal prepared elsewhere), keeping distinct floor and ceiling orientations, runs the moves once without portals, and pairs each portal that run comes near with every other: only a portal on the body's way is ever entered. For each pair it resets to the standing point, opens both, runs the moves, and reports the crossings and where the body ends: its level, cell, and platform, or `DIES`. Pairs that end as the moves do without portals, and pairs the game refuses because the two portals would overlap, are counted, not listed; health lost on the way is named. Repeatable `--entry` and `--exit` select surfaces for either end; `--goal L1:2,2:6,4` marks the pairs that end standing in that rectangle of cells, on a plate or not. A puzzle step is sound when its intended pair is the only one at the goal. A sweep tries the moves it is given: a shortcut by other moves is not covered.

`--after <route.json>:52`, on `shots` and both kinds of sweep, starts every attempt with the state a route has after its step 52, as `proof` numbers the steps: the route's equipment, switches, keys, and health, with the body set down at the standing point or beside the entry portal by `teleport`. Sweeps clear the route's portals before probing, running the baseline, or placing each candidate pair; `shots` keeps them. Without it an attempt starts fresh, as if the map had just loaded. A late step of a route is checked this way, and so is a goal that equipment or a switch from earlier changes. The route replays before every attempt, so a short route, or a step early in a long one, keeps a sweep quick.

`sweep --from L1:4,6 --walk-in --goal ...` asks the question a sweep of chosen moves misses: does a body that builds no speed get to the goal through some pair? For every portal in reach as the exit, it walks onto, steps into, and drops from a jump's height into a floor portal, and walks and runs and jumps into a wall portal that has a floor at its foot, each from a start beside that portal, and prints `SHORTCUT` with every entry that ends at the goal. Where a slow body leaves depends on the exit and the way in, so one roomy floor portal and one wall portal stand for all entries unless `--entry` names them, in which case every named entry is tried. A goal passes when none reaches it; with portals on almost every surface that takes metal on the goal and on what lies above and beside it.

## Proving a route

```sh
cargo run --release -- --map <map>                  # play it
cargo run --release -- --experiment <map>           # headless, report on stdout
cargo run --release -- --play-experiment <map>      # step through it
cargo test --release -p cuboid-wars                 # route and runner tests
```

A map's `experiment.json` sits beside its `layout.json` and `settings.json`, and the map's name finds it; a path ending in `.json` runs any other script. The map's README holds the walkthrough. Headless mode needs no window, listener, or registry entry; invalid scripts are process errors, failed checks are report entries.

Playback starts paused: Space plays or pauses, Enter runs one action, R restarts, Esc opens the menu. The view is the one a player would have: level along the direction of travel, starting on the script's first move, and on the target from an `aim` until the next move. Mouse look, zoom, and V inspect a paused scene; the next control or tick eases the view back. Continuous playback holds briefly after an `aim`, a `portal`, and a `place` so the view arrives and the result shows. Pausing and holding stop the owner and the server alike, so waiting adds no ticks. Playback re-executes the script; it is not a recording, and randomness is unseeded.

### Script

`gameplay`, `settings`, and `layout` are paths relative to the script; `spawn` is the initial feet position and may be airborne. Equipment, health, checkpoints, and respawn policy come from the map files.

| Action | Behavior |
| --- | --- |
| `move`, `direction: [x, z]`, `ticks: N` | Hold a direction for N ticks; magnitude is ignored, `[0, 0]` holds nothing. Optional `crouch: true`; `jump: true` attempts one jump on the first tick. |
| `walk_to`, `target: [x, z]`, `ticks: N` | Walk to a point on the ground and stop on it, in at most N ticks: `arrived`, or `short` with the distance left when something stops the body. Where a walk ends does not change with the movement numbers, so a route walks with it and keeps `move` for run-ups, jumps, and steering in the air. |
| `advance`, `ticks: N` | N ticks with no movement input. |
| `aim`, `target: [x, y, z]` | Aim from the eye at a world point, no time passes. |
| `portal`, `end: "a"` or `"b"` | A portal shot with the current aim under the normal placement rules. |
| `place`, `end`, `eye: [x, y, z]`, `target: [x, y, z]` | The portal a shot from `eye` at `target` opens, wherever the player stands: the same rules, cooldown, and tick as `portal`. |
| `probe`, `targets: [[x, y, z], ...]`, optional `eye` | What a portal shot at each target would do, from `eye` or the player's: where the ray lands, and `placed` with the aperture, `no_fit`, `incompatible_material`, or `no_surface`. Changes nothing, no time passes. |
| `check`, `min`, `max` | The living player's feet lie inside inclusive world bounds, grounded unless `grounded: false`. |
| `inspect` | Record state, no time passes. |
| `clear_portals` | Close the player's portals, keeping equipment, cooldown, and all other session state; no time passes. |
| `reset`, optional `spawn: [x, y, z]` | Recreate server and owner at `spawn`, or at the scripted one; aim returns to +Z. A `spawn` inside geometry is `rejected` and the session stays as it was. |
| `teleport`, `feet: [x, y, z]` | Set the living body down at rest there, keeping everything else; the next report carries it to the server. A spot inside geometry, or a dead player, is `rejected` and changes nothing. |
| `fire` | One ordinary projectile with the current aim. |

A direction starts in world space, resolves against the aim into forward and sideways input, and turns with the player through portals. Input is released at the end of a `move`; a second `move` can steer in midair where the map allows. Jumps use the game's support checks, so a jump requested in flight makes no double jump. Death interrupts a `move`; `advance` waits out the respawn. A fired shot, blocked muzzle, submitted portal or placement, or fizzle consumes one tick; rejected preconditions, invalid placement, and overlap consume none.

### Report and scope

The report holds the `initial` state and a `steps` array: each action, its result, its events, and the resulting state, including the owner's position and the last one the server adopted, velocities, stance, support, health, equipment, checkpoint, switches, and open fields. Per-tick `player_step` records, portal crossings, landings, fall damage, checkpoints, and deaths explain an outcome; failed checks report `outside_region`, `not_grounded`, or `player_dead`.

The runner owns its player like the rendered client, through the same owner tick, planner, portal traversal, and outcome reports, and sends ordinary `CMove`/`CMoveOutcome` messages to a real server schedule. Observations come from each completed tick without interpolation. Carrier maps are rejected; missiles, route search, and measures of fun are not implemented.
