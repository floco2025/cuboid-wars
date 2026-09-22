# Movement and portal experiments

For the longer traversal course, play **Portal Relay**:

```sh
cargo run --release -- --map portal_relay --look 270,-10
```

It connects a wall-portal turn, a jump onto a bridge switch, a horizontal fling,
a vertical launch with an air catch, and a diagonal finish jump. Five checkpoints
save progress, and the finish plate starts fireworks. The portal gun is available
from spawn. See the [course guide](config/server/maps/portal_relay/README.md)
for hints and validation.

To play the movement example yourself:

```sh
cargo run --release -- --map portal_movement --look 90,-20
```

This opens the normal game window at the map's starting checkpoint. You control
the player; the scripted actions are not executed.
Window options such as `--windowed` and `--resolution 1280x720` work normally.

The maps live in `config/server/maps/` and are registered alongside the other
maps. Open them in the normal editor:

```sh
python3 tools/editor.py portal_relay
python3 tools/editor.py portal_movement
python3 tools/editor.py portal_turret
```

Each map's `experiment.json` lives beside its `layout.json` and `settings.json`
in `config/server/maps/<name>/`. The scripts reference those adjacent files and
the shared `../../gameplay.json`. Both experiment modes use the same layout and
settings files that the editor edits.

In the short movement example, approach the starting ledge, place portal A on
the small floor below with the left mouse button, and portal B on the high wall
across from you with the right mouse button. Walk off toward the floor portal,
then release movement while falling.
The exit launches you back across the gap toward the lower checkpoint platform.
WASD moves, Shift runs, Space jumps, and Q cycles weapons if needed.

Run a JSON route script against a map without opening a window:

```sh
cargo run --release -- --experiment config/server/maps/portal_movement/experiment.json > /tmp/portal-report.json
```

The report goes to stdout. Invalid scripts, maps, or unsupported simulation
conditions produce an error on stderr and a nonzero exit status. Failed route
checks and unsuccessful actions appear in the report; they are not process
errors. No rendered client, listener, or map registry entry is required.

## Watch an experiment

```sh
cargo run --release -- --play-experiment config/server/maps/portal_relay/experiment.json --look 270,-10
```

The window starts paused at the scripted spawn. **Space** starts or pauses
continuous playback of the entire sequence, including mid-jump. Playback runs
at the configured simulation rate and stops at the end of the script. **Enter**
runs just the next action, or finishes the current action if one is in progress,
then pauses. **R** restarts the entire experiment and leaves it paused.
**Esc** opens/closes the settings menu and pauses playback; after closing it,
use Space or Enter to continue. **Shift+Esc** releases the cursor without
opening the menu. The overlay shows the next/current action, completed action count,
simulation tick, and the last result, including failed checks and rejected shots.
An execution error remains visible until restart. Failed checks do not prevent
you from stepping through later actions.

Mouse look, wheel zoom, and **V** camera switching work while paused. They move
only the inspection camera; scripted movement and weapon aim stay independent.
Use `--look` to choose the initial view. WASD and weapon buttons do not control
the experiment. Use `--map portal_relay` for ordinary manual play.

The window and headless mode share one action executor. Long actions yield after
each fixed tick; pausing stops both the movement owner and the server, including
cooldowns, respawn timers, and actor movement. The renderer observes the completed
state directly without network interpolation. Changing frame rate or waiting
between actions does not add simulation ticks. This executes the script again;
it is not a recording, and actor randomness is not yet seeded.

## Movement example

The player first approaches the edge of the starting platform, places a floor
portal below the ledge and a wall portal above a gap, then walks off the ledge,
falls through the floor portal, launches out of the wall portal, and lands on a
separate platform with checkpoint 1. The last
`check` requires a living, grounded player inside that platform's bounds.

The trace shows the falling velocity becoming horizontal exit momentum, the
landing, and the server's checkpoint claim. Removing the second portal fails the
route. Moving the exit beside the destination platform produces a real crossing
but misses the landing. These are physical route checks, not a judgment of how
interesting or discoverable the challenge is.

Placement uses the actual collision surface. The starting ledge extends beyond
its authored cell center spacing; standing too far from its edge blocks the shot
to the floor below. Wall thickness likewise matters when aiming diagonally.
Inspect reported portal positions rather than assuming a shot reaches its target.

## Script

`gameplay`, `settings`, and `layout` are file paths relative to the script.
`gameplay` supplies the ordinary game defaults; `settings` is a normal per-map
override, and `layout` uses the editor's map format and validation. `spawn` is the
initial feet position in world meters and may be airborne. Equipment, health,
checkpoints, and respawn policy come from the map files.

These actions can be combined to test an approach, a jump, a crossing, and a landing:

```json
[
  { "action": "aim", "target": [-6, 0, -6] },
  { "action": "portal", "end": "a" },
  { "action": "advance", "ticks": 4 },
  { "action": "move", "direction": [1, 0], "ticks": 20, "run": true, "jump": true },
  { "action": "advance", "ticks": 30 },
  { "action": "check", "min": [-20, 4.3, -8], "max": [-12, 4.6, -4] }
]
```

Use `config/server/maps/portal_movement/experiment.json` for a complete executable
script; the fragment above only illustrates the action format.

| Action | Behavior |
| --- | --- |
| `move`, `direction: [x, z]`, `ticks: N` | Hold a walking direction for N ticks. Magnitude does not change speed; `[0, 0]` holds no horizontal input. Optional `run: true` uses running speed; `jump: true` attempts one jump on the first tick. |
| `advance`, `ticks: N` | Simulate N ticks with no movement input. Gravity, momentum, and actors keep advancing. |
| `aim`, `target: [x, y, z]` | Aim from the player's eye at a world point, without advancing time. |
| `portal`, `end: "a"` or `"b"` | Attempt a portal shot using the current aim and normal placement rules. |
| `check`, `min`, `max` | Test whether the living player's feet lie inside inclusive world bounds. Requires ground support by default; `grounded: false` permits an airborne waypoint. |
| `inspect` | Record state without advancing time. |
| `reset` | Recreate the server and owner, return to the scripted spawn, and clear movement, checkpoints, shots, and portals. Aim returns to +Z. |
| `fire` | Fire one ordinary projectile using the current aim. |

A direction starts in world space and turns through portals with the player's
held input. At the end of `move`, that input is released; airborne momentum
continues. A second move can change direction in midair. Jumping uses the game's
support checks, so requesting another jump in flight does not create a double
jump. Death interrupts a `move`; `advance` can wait for the normal respawn.

A fired shot, blocked muzzle, submitted portal, or material fizzle consumes one
tick, including player physics. Rejected preconditions, invalid placement, and
portal overlap consume no time. Use `advance` to wait out the shared weapon
cooldown. `aim` sets weapon direction, not a camera orbit or body rotation;
movement sets body heading and portal crossings transform the aim.

## Report and simulation scope

The JSON contains `initial` state and a `steps` array. Every step records the
action, its result, events, and resulting state. Player state includes the owner's
position, the last position adopted by the server, vertical velocity, momentum,
knockback, support, health, equipment, and checkpoint. These positions can differ
at lower movement report rates; the owner remains authoritative for its motion.
`active_switches` and `open_fields` contain authored names: an open field is off,
so a light bridge belonging to it cannot support the player. `fireworks_started`
records the server's celebration cue without its random presentation seed.

`player_step` records each tick's positions, combined control/momentum velocity,
and geometry or actor blocking. That velocity describes the request; a blocked
body may not move that far. `player_portal_crossing` records entry/exit frames and
velocity before/after transit. `player_landed`, `player_fall_damage`,
`checkpoint_reached`, and death/relocation events explain the outcome. A grounded
`check` waits for support from a tick after transit, since the transit tick's motor
result describes the entrance. Failed checks report `outside_region`,
`not_grounded`, or `player_dead`.

The normal client and runner share movement planning and body blocking,
movement/outcome report construction, portal traversal, and view mapping. The
runner owns its player's position and sends ordinary `CMove`/`CMoveOutcome`
messages. It accepts server relocations and impulses without treating ordinary
server observations as movement corrections. The regular server schedule handles
actors, equipment, checkpoints, pressure plates, damage, and death. Observations
come directly from each completed server tick without network/interpolation delay.
Projectile flight also uses the normal client code.

Carrier maps are rejected for now. Missile flight, route search, and evaluations
of fun/discoverability are not implemented. Reset restores
the setup, not a global random seed: actor headings, random choices, and background
navigation may vary. The movement example has no actors or random items and its
regression test checks identical traces across runs.

The earlier shooting example remains at
`config/server/maps/portal_turret/experiment.json` as a secondary check: direct
fire is blocked and a shot through the portal pair kills the turret. Projectile
samples and hit/death notifications remain in the report.

Run the route, encounter, and CLI regression tests with:

```sh
cargo test --release -p cuboid-wars experiment
```
