# Map authoring for AIs

`tools/mapauthor.py` is how an AI composes a map, looks at it, measures it,
and proves a route, without a renderer. Every rule and number comes from the
game: the builder validates with `map_core`, flights run the game's own
step, funnel, and portal hop, and the proof is the headless experiment
runner. An AI-authored map keeps its `build.py` beside its `layout.json`;
that script is the map's source, and a human who edits the layout in the
editor updates or deletes it.

The first command builds the Rust map library (`cargo build --release -p map_core_py`), which takes a few minutes once.

```sh
python3 tools/mapauthor.py build <map>                 # run config/server/maps/<map>/build.py
python3 tools/mapauthor.py describe <map> [--level N] [--plan | --summary]
python3 tools/mapauthor.py where <map> L4:9.5,17.5 | --world -46,8.8,-30
python3 tools/mapauthor.py surface <map> wall:L4:6,16:W | floor:L2:37,42
python3 tools/mapauthor.py jump <map> --from L4:11,17:E [--walk] [--late 0.1] [--air-control]
python3 tools/mapauthor.py fling <map> --from L4:6,17:W --into --entry wall:L4:6,17:W --exit wall:L6:20,23:S
python3 tools/mapauthor.py ranges <map>
cargo run --release -- --experiment config/server/maps/<map>/experiment.json | python3 tools/mapauthor.py proof <map> -
```

## Workflow

1. Register the map in `config/server/gameplay.json::maps` and write its `settings.json` (copy `portal_primer`'s: 2 m cells, 2.2 m levels, a `solid` and a `portal` texture alias). The tools never write either file.
2. Write `build.py` (below), run `build`, read the summary it prints.
3. `describe` for the plan of each level; `surface`, `jump`, `fling`, and `ranges` for the physics of each gap and portal pair. Place landings where a flight comes down, not where it looks right.
4. Write `experiment.json` beside the layout (format in `EXPERIMENTS.md`), run it, read `proof`. Every `aim`, `check`, and `spawn` is in world metres; `describe` and `where` give them.
5. Iterate until the route passes, then pin it in `src/experiment/tests/` like `primer.rs`, on `scenario`, which pins the movement rates the course was proved against: the completion run and the failures the course is built on (a missing portal, a skipped pickup, a plate not pressed). A route the runner cannot finish because the player died ends the run at the next `aim`; truncate the script to see the report up to there.
6. `python3 tools/editor.py <map>` opens the result; Check Map must be clean.

## Coordinates

Cells and levels everywhere in the tools: `(col, row)` with columns along x and rows along z, rects end-exclusive, level L's floor top at `L * level_height`. The grid is centred on the world origin, so world x = `col * cell - cols * cell / 2`, the same for z, and feet y = `L * 2.2`. Scripts take world metres; `where` converts both ways. A takeoff `L4:11,17:E` leaves cell (11, 17) on level 4 over its east edge; a surface `wall:L4:6,17:W` is the wall on the west side of cell (6, 17) with its portal facing that cell, and `floor:L2:37,42` a floor point at cell coordinates (37, 42), which is the corner shared by cells 36 and 37.

## Rules of thumb at the portal-course geometry

Cell 2.0 m, level 2.2 m, floors and walls 0.2 m thick; a wall section is 2.0 m tall. Defaults: move 8 m/s, jump 12 m/s (apex 2.88 m, so a jump clears one storey and not two), gravity 25, low gravity 5 (apex 14.4 m), the speed pickup ×1.5, air braking 5 m/s² with input released. Fall damage starts at an 8 m equivalent drop and is lethal at 15 m; a 4.4 m drop costs nothing. The body is 0.6 m wide and 1.8 m tall, eyes at 1.62 m. The funnel pulls a falling body onto a floor portal from 0.6 m plus 0.8 m per second still to fall, and never changes its velocity, so a fling leaves at the angle it entered.

Low gravity carries a jump tens of metres and lands softly from any height, so a goal it must not reach needs walls and a roof entered through an `eraser` edge; eraser pickups can be walked around.

Do not trust these for a gap: run `ranges` once per map and `jump` at the actual edge. Released input brakes hard in the air; a player crossing a gap holds forward, which is the `held` row and the `--air-control` lines.

## Portal backing

A portal is 1.4 × 2.6 m. A wall portal needs two stacked sections on the same edge (`portal_wall` writes both) and nothing standing in front of the upper one; a floor portal needs two portalable cells side by side with no wall between them (`portal_floor` writes a 2×2 pad) and no pressure plate within 1.2 m. A surface's normal fixes the exit direction; where the portal is shot from only turns a floor portal by quarter turns. Materials come from the map's `settings.json::textures` aliases and their `portalable` flag. `surface` and the summary's "portal-ready" list judge all of this; the runner's `portal` action is the final word.

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

Placement is `at=(col, row)` or exactly one of `east_of`, `west_of`, `north_of`, `south_of` naming an earlier piece, with `gap` cells between and `shift` cells along the shared edge; `level` is a number or a piece's name, with `up` and `down`. Pieces: `platform`, `portal_floor`, `portal_wall(name, level, at, side, length=1)`, `wall(level, start, end, material=None, storeys=1)` between grid points, `bridge(..., field=)`, `barrier(level, start, end, field=)`, `eraser(level, start, end)`, `ramp(name, lower_level, size, direction=, levels=1, shape="solid")` where `direction` is the side it rises toward and the slope must be climbable, `ladder(lower_level, landing, side, levels=1)` on a side of its top landing cell, `checkpoint(number, level, size, ...)`, `plate(level, at, switch=)`, `item(type, level, at, field=None)`, `switch(id, activation=, reset=, held=, color=)`, `field(id, switch=, initially_on=, color=)`, `fireworks(switch)`. `save()` normalizes, validates, raises `BuildError` with the validator's messages, writes the layout, and prints the summary. Out of scope: terrain, grounds, nested maps, actor zones, lights, random items, quests.

## Reading the views

The plan draws one character per cell with the walls between them; the legend is printed with it. The summary names each platform `L<level>.<letter>` with its grid and world bounds and what it holds, the nearest neighbour in each direction with the gap in cells and metres and the storey difference, every portal-ready surface with its world centre and normal, the portalable surfaces that are not ready and why, ramps with their slope, ladders, barriers, erasers, switches with their plates, and fields.

`jump` and `fling` print, per pickup scenario, where the flight crosses each floor height, in cells and world metres, with the fall damage and whether a floor is there; the first floor is the landing. With `--air-control` they add how far holding a direction carries on each level. `fling --into` runs into the entry wall from the takeoff cell instead of jumping off an edge.

`proof` prints one line per action with its result and the events that matter (crossings, landings, checkpoints, pickups, erasure, deaths, portal results), the body's end position as world metres and a cell, and a footer with the checks passed. A passing script shows a route exists; it does not show that the route is necessary or readable, which `EXPERIMENTS.md` covers.
