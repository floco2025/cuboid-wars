# Movement and portal experiments

## Goal

AI-generated complex maps that are fun to play, starting with single-player
movement and portal traversal. Earlier generated maps were boring and primitive;
appearance, enemies, and decoration do not address that. Complexity comes from
connected decisions: preparing a route, choosing an exit surface, building the
right entry velocity, and changing equipment at the right point. More platforms
and repeated jumps alone are insufficient.

The maps and tools here are groundwork: authored courses, scripts that exercise
the real game simulation, and graphical playback for inspecting them. There is
no map generator, route search, or evaluator of fun. A script reaching the
finish establishes a working route; only player feedback establishes whether it
is readable and satisfying.

## Design decisions to preserve

- **Portal preparation from another location.** One portal must be set up from
  a different vantage before returning to the entrance. A surface's normal
  fixes its exit direction; the shooting position does not rotate it.
- **Ramp angles as a decision.** Inclined portal surfaces with different
  trajectories toward a shared destination; the wrong ones fail for
  understandable physical reasons, not by visibly leading nowhere.
- **Required changes of equipment.** Speed and low-gravity pickups as
  prerequisites, and the `equipment_eraser` so erasure can be unavoidable
  mid-flight. Erasure keeps existing velocity; losing speed changes the target
  speed, losing low gravity changes gravity at once.
- **Recovery after mistakes.** Losing a required boost leaves a way to refill
  and retry, and every checkpoint has usable equipment and a discoverable route
  onward. Test recovery as well as the intended solution.
- **Necessary traversal.** Check that ordinary jumps, drops, other portal
  placements, or air steering do not bypass a puzzle. A passing script does not
  show its portals or pickups are necessary.
- **Puzzle contract.** Main progression uses momentum from an ordinary takeoff
  or fall, portal orientation, and ordinary steering, with W released. Expert
  air control may reward optional routes only. Courses may zero the air rates
  to keep launch momentum; Obby zeroes air acceleration too, so its routes
  cannot depend on airborne corrections.
- **Visibility.** Landing platforms and portal pads sit far enough out that a
  player can see and aim at them from a safe approach, never from the lip
  looking straight down. Judge the gap against the drop and the view, then
  verify the traversal.
- **2 m floor and wall sections** for Relay and Choices, with 2.2 m levels and
  normal player and portal sizes; assemble cells where a portal needs more
  backing. A course choice, not a grid rule.
- **Obby is edited by hand.** Do not change its layout or retune its movement
  overrides on the author's behalf. The generated courses are retuned for the
  current movement in the Fixes entry of [TODO.md](TODO.md); keep their
  failing route assertions visible rather than weakening them.

## Run and watch

```sh
cargo run --release -- --map portal_choices --look 90,-10                                   # play it
cargo run --release -- --experiment config/server/maps/portal_movement/experiment.json      # headless, report on stdout
cargo run --release -- --play-experiment config/server/maps/portal_relay/experiment.json    # step through it
cargo test --release -p cuboid-wars                                                          # route and runner tests
```

Each map's `experiment.json` sits beside its `layout.json` and `settings.json`;
the map READMEs hold the walkthroughs. `python3 tools/mapauthor.py proof <map> -`
reads a report from stdin as one line per action, and the rest of
`tools/mapauthor.py` builds, draws, and measures a map before it is proved
(`tools/map_author/README.md`). Headless mode needs no window, listener,
or registry entry; invalid scripts are process errors, failed checks are report
entries. Playback starts paused: Space plays or pauses, Enter runs one action,
R restarts, Esc opens the menu. The view is the one a player would have: level
along the direction of travel, starting on the script's first move, and on the
target from an `aim` until the next move. Mouse look, zoom, and V inspect a
paused scene; the next control or tick eases the view back. Continuous playback
holds briefly after an `aim` and a `portal` so the view arrives and the result
shows. Pausing and holding stop the owner and the server alike, so waiting adds
no ticks.
It re-executes the script; it is not a recording, and randomness is unseeded.

## Script

`gameplay`, `settings`, and `layout` are paths relative to the script; `spawn`
is the initial feet position and may be airborne. Equipment, health,
checkpoints, and respawn policy come from the map files.

| Action | Behavior |
| --- | --- |
| `move`, `direction: [x, z]`, `ticks: N` | Hold a direction for N ticks; magnitude is ignored, `[0, 0]` holds nothing. Optional `crouch: true`; `jump: true` attempts one jump on the first tick. |
| `advance`, `ticks: N` | N ticks with no movement input. |
| `aim`, `target: [x, y, z]` | Aim from the eye at a world point, no time passes. |
| `portal`, `end: "a"` or `"b"` | A portal shot with the current aim under the normal placement rules. |
| `check`, `min`, `max` | The living player's feet lie inside inclusive world bounds, grounded unless `grounded: false`. |
| `inspect` | Record state, no time passes. |
| `reset` | Recreate server and owner at the scripted spawn; aim returns to +Z. |
| `fire` | One ordinary projectile with the current aim. |

A direction starts in world space, resolves against the aim into forward and
sideways input, and turns with the player through portals. Input is released at
the end of a `move`; a second `move` can steer in midair where the map allows.
Jumps use the game's support checks, so a jump requested in flight makes no
double jump.
Death interrupts a `move`; `advance` waits out the respawn. A fired shot,
blocked muzzle, submitted portal, or fizzle consumes one tick; rejected
preconditions, invalid placement, and overlap consume none.

## Report and scope

The report holds the `initial` state and a `steps` array: each action, its
result, its events, and the resulting state, including the owner's position
and the last one the server adopted, velocities, stance, support, health,
equipment, checkpoint, switches, and open fields. Per-tick `player_step`
records, portal crossings, landings, fall damage, checkpoints, and deaths
explain an outcome; failed checks report `outside_region`, `not_grounded`, or
`player_dead`.

The runner owns its player like the rendered client, through the same owner
tick, planner, portal traversal, and outcome reports, and sends ordinary
`CMove`/`CMoveOutcome` messages to a real server schedule. Observations come
from each completed tick without interpolation. Carrier maps are rejected;
missiles, route search, and measures of fun are not implemented.
`portal_turret/experiment.json` is the shooting example: direct fire is
blocked and a shot through the pair kills the turret.

## References

Valve's [Source SDK movement](https://github.com/ValveSoftware/source-sdk-2013/blob/master/src/game/shared/gamemovement.cpp)
is the base acceleration and friction model, and the Portal 2 community's
[strafe prediction](https://github.com/p2sr/SourceAutoRecord/blob/master/src/Features/Tas/TasTools/StrafeTool.cpp)
gave the initial profile at 0.025 m per Source unit. Ground and air rates were
then separated and the speed limits changed for this game; the SDK is not a
specification of retail Portal 2.
