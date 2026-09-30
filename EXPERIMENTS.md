# Movement and portal experiments

## Goal and priorities

The goal is **AI-generated complex maps that are fun to play**, starting with
single-player movement and portal traversal. Earlier generated maps were boring,
simplistic, and primitive. Appearance is not a priority at this stage. Shooting
encounters are secondary; adding enemies or decoration does not address the
problem these experiments are meant to solve.

The current maps and tools are groundwork for that goal. We have authored
courses, scripts that exercise the real game simulation, and graphical playback
for inspecting those scripts. We do not yet have an automatic map generator,
route search, or an evaluator of fun. A script reaching the finish establishes
a working route; player feedback must establish whether that route is readable,
interesting, and satisfying.

Complexity should come from connected decisions: preparing a route, choosing an
exit surface, building the right entry velocity, and changing equipment at the
right point. More platforms and repeated jumps alone are insufficient.

## Current handoff: movement first, Hotel playtest

The rollout is deliberately staged. **Stage 1 implements the new movement model;
Hotel playtesting and tuning are in progress.** Stop for movement feedback before proceeding:

1. Replace movement and integrate input, crouching, cameras, hitboxes, networking,
   portal traversal, and the shared headless owner. Map changes in this stage are
   initially mechanical only: `walk_speed` becomes `move_speed`, `run_speed`
   disappears, and scripts no longer have a `run` flag. Shared movement defaults
   are then tuned through Hotel playtesting. Layouts and route timings stay intact.
2. After the Hotel playtest, update the editor to use the same Rust movement and
   traversal code. Batch complete trajectory simulations across PyO3 instead of
   making a Python/Rust call each tick. Derive entry velocity from the authored
   takeoff and route, rotate it through the portals, and show released-input and
   held-W reach when they differ. Include ramp exits, crouching, pickups, erasers,
   switches, and carriers in the simulation; profile repeated previews.
3. The user will adapt **Obby manually**, using those editor tools. Do not change
   Obby's layout or retune its movement overrides on their behalf.
4. Retune the generated maps and their experiment scripts/tests for the accepted
   movement. Restore completion, shortcut, pickup-necessity, recovery, timing,
   and visibility checks. Historical successes below do not establish reach under
   the new model.

Play the current movement in Hotel with:

```sh
cargo run --release -- --map hotel
```

WASD moves at one normal speed, Space jumps, and holding Ctrl crouches. Shift no
longer sprints. Try start/stop and reversal on flat ground, jumps with W released
and held, strafing, stairs/ramps, low-ceiling crouching, and moving platforms.
Then compare portal flings with W released and held along the outgoing velocity.
There are no separate air-control rules before and after a portal.

The first Hotel playtest found excessive sideways carry when turning on the
ground, slow travel, and low-gravity jumps that could no longer reach the roof.
The initial reference tuning reduced normal speed from 6 to 4.375 m/s and
low-gravity jump rise from about 14.4 to 3.4 m. The shared normal speed is now
**9 m/s**, the former run speed; Hotel inherits it without a map override. At
the user's request, shared defaults restore both former jump arcs: 12 m/s takeoff with
25 m/s² ordinary gravity or 5 m/s² low gravity. Ground control now separates
acceleration, braking, and sideways grip, allowing slow starts with fast stops.
Ground and air now share this model and use direct m/s² rates; the former always-on ground friction and minimum
braking speed have been removed. Persistent velocity and directional air control
remain the movement model. Repeat the
Hotel playtest for ground grip, travel pace, and ground-to-roof boosted jumps;
these tuning changes have not yet been confirmed by a human playtest.

Air acceleration now targets the available movement speed: 9 m/s by default,
including Hotel, scaled by speed pickups. The former separate air cap and
high-speed world-axis input restriction were removed after the Hotel playtest.
Air acceleration no longer multiplies movement speed: the default 20 means
20 m/s², matching ground acceleration. Default air stopping and lateral braking
match ground at 30 and 40 m/s² respectively. Releasing input brakes horizontal
motion, and changing direction removes unwanted sideways drift. A launch above
the requested speed also slows toward that speed while input is held. Setting
both air braking rates to zero preserves momentum; Obby additionally sets air
acceleration to zero, disabling manual air steering. Gravity and automatic portal
funneling remain active. Hotel inherits the defaults without a map override.
`move_speed_power_up` is a multiplier (1.5 by default: 9 becomes 13.5 m/s),
independent of `jump_speed` and all acceleration/deceleration rates.

**Known stage boundary:** the editor's Jump Path preview runs the shared Rust
movement, funnel, and portal hop through `map_core::preview` (open air,
full-speed takeoff, released input, optional steering ranges); the remaining
preview scope is in TODO.md. Generated-course route assertions currently fail; their scripts and
walkthroughs describe the previous movement. Keep the assertions visible until
stage 4 retunes the courses; do not weaken them to accept failed routes.

Validation after independent air braking, matching ground/air units, and the
movement-speed multiplier rename: the client (544), common physics/types (382), server (687), map core (23),
and 34 runner/CLI tests pass; all 483 editor tests pass. Nineteen old
Movement, Relay, and Choices route assertions fail and remain enabled, including
one additional Relay timing assertion under the new ground controller. Runner
fixtures now pin movement rates so subsequent Hotel tuning does not change their
expectations. Shared regressions cover slow acceleration with fast stopping at
30/60/120 Hz, independent sideways grip, braking before reversal, gradual slowing
to lower ground targets, stationary takeoffs followed by forward input,
movement-speed limits with pickups, equal physical rates across ground and air,
independent air stopping/lateral braking, ballistic all-zero air control, blast
decay without reverse drift, coasting with braking disabled, and steering at
multiple headings. Release braking and sideways corrections leave vertical gravity unchanged.
Funnel regressions cover edge/corner entries, terminal falls, zero/low gravity,
misaligned loops, steering release, obstructed approaches, moving/sloping gates,
and independence from movement tuning at 30/60/120 Hz. Graphical pacing,
continuous playback, pause/resume, reset, and input isolation are covered separately.
A bootstrap regression constructs portal and fizzle assets from received sizes
without a preinstalled gameplay resource. Formatting and workspace Clippy pass.

## Design decisions to preserve

- **Portal preparation from another location.** Require the player to set up
  one portal from a different vantage before returning to the entrance. Avoid
  always allowing both portals to be placed from the takeoff position. The
  original aim was to make preparation matter to the eventual exit direction.
  In Portal Choices, the first puzzle currently enforces access to the exit
  surface through sightlines. A wall or ramp's normal determines its exit
  direction; changing the shooting position does not rotate that normal.
- **Ramp angles as a decision.** Offer inclined portal surfaces with different
  trajectories toward a shared, plausible destination. Alternatives should
  fail for understandable physical reasons, such as excessive height or speed.
  Surfaces that visibly lead nowhere are weak choices.
- **Required changes of equipment.** Use speed and low-gravity pickups as route
  prerequisites. The `equipment_eraser` pickup was added so erasure can be
  unavoidable during a jump or portal exit, including above a floor. It clears
  collected abilities and ammunition while preserving always-active abilities,
  keys, health, and checkpoint progress. It preserves stored launch momentum
  and vertical velocity. Removing speed changes the target speed and available
  acceleration without deleting existing velocity; low-gravity removal changes
  gravity immediately. Placement and gravity after collection can determine the landing.
- **Recovery after mistakes.** Losing a required boost must leave a way to
  replenish it and retry. Checkpoints need usable equipment and a discoverable
  route onward. Test recovery as well as the intended uninterrupted solution.
- **Necessary traversal.** Check whether ordinary jumps, drops, alternative
  portal placements, or air steering bypass a puzzle's prerequisites. Successful
  execution of the intended script alone does not show that its portals or
  pickups are necessary.

### Visibility and placement

When authoring these maps, keep landing platforms and portal pads far enough
out from their takeoff platforms that players can see and aim at them from a
safe approach position. Never require standing on the lip and looking straight
down over the edge. Judge the gap against the drop height and the player's view,
then verify the traversal; one empty grid cell is not a general visibility rule.
Tune visibility and reach together so moving a pad outward does not create an
unreasonably precise or unreachable jump.

We settled on **2 m horizontal floor and wall sections** for Portal Relay and
Portal Choices, rather than a much finer grid. This permits useful offsets
without excessive authoring detail. Their levels are 2.2 m high; player and
portal dimensions retain their normal physical sizes. Assemble adjacent floor
cells or stacked wall sections where a portal needs more backing. For now, map
authors can provide sufficient space themselves; editor guidance for backing
size and front clearance is an enhancement in [TODO.md](TODO.md). Runtime portal
placement still checks the actual geometry. These dimensions are course choices,
not a change to every map's grid.

## Course history before the movement replacement

**[Portal Choices](config/server/maps/portal_choices/README.md)** is the latest
experiment combining these ideas. The following playtest findings and route
results were recorded with the previous movement model. Keep **Portal Relay** as the simpler reference
course. `portal_movement` and `portal_turret` are small mechanics examples.
The map README holds the player walkthrough; its adjacent `experiment.json`
holds an executable solution.

Choices links separate-position portal preparation, a speed-powered ramp launch,
erasure, a low-gravity jump to checkpoint 3, and a deep floor-portal drop. The
final ramp and suspended eraser turn that drop into a landing at checkpoint 4.
There are no enemies.

Player feedback drove these changes and should guide further iterations:

- The first portal could originally be skipped by jumping down from the
  balcony. The balcony was set back, and direct jump/drop checks were added.
- The side ramps were obvious dead ends. All three final ramps now face the
  same finish. The tested wrong arcs cross above it and overshoot. This does
  not prove that every placement or steered alternative fails.
- Losing speed around the second portal puzzle could strand the player. The
  entry runway now has a refill, and a lower retry deck provides another speed
  pickup and portal entrance, with a return route from the upper platform.
- The player confirmed that checkpoint 2 to checkpoint 3 works without the
  speed pickup. Low gravity and ordinary sprinting are the intended combination.
- The final approach means **checkpoint 3 to the lower floor portal**, not the
  flight from its exit to the finish. The player initially reported that jump
  as impossible without speed, then confirmed making it by sprinting. Keep
  **Shift sprinting** distinct from the **speed pickup** in instructions and
  diagnosis; do not record this as a confirmed failure while sprinting.
- Lower pads were too close below their takeoff ledges to see comfortably. The
  first pad now has a 6 m horizontal edge gap. The final pad was moved outward,
  then its gap was reduced from 30 m to **22 m** to improve reach while preserving
  visibility. Manual confirmation of that latest spacing is still outstanding.

The [Choices regression tests](src/experiment/tests/choices.rs) cover completion,
selected shortcuts and wrong equipment, repeatable recovery, and variation in
portal placement and approach timing. With all speed pickups removed, the final
section is checked with walking-speed jumps, several sprint takeoffs, and a
run-off without jumping. Low gravity is still required for those approaches.
Sightline checks submit real portal shots from grounded positions with at most
60 degrees of downward aim.

These are simulated checks, not a rendered camera or manual-input playtest.
The [playback tests](src/experiment/tests/playback.rs) compare scripted execution
at different frame rates, but do not establish how discovery, third-person
aiming, or recovery feels. The lower pads are still small targets; the scripted
route releases movement when aligned above the entrance. The older reference
maps also need review against the visibility rule above; their existence is
not evidence that all their ledges meet it.

## Movement model and puzzle contract

A human playtest exposed a misleading requirement: some Obby portal exits needed
continued W input to reach the landing. The previous controller added full input
speed instantly on top of separately stored portal momentum; releasing W removed
that extra velocity immediately. We replaced this with persistent horizontal
velocity and Portal-inspired air acceleration. Portal traversal rotates actual velocity
once. The same air controller then operates everywhere.

The reference profile scales Source units by 0.025 m (a 72-unit standing hull
becomes 1.8 m). Hotel feedback led to faster ground response and restored the
game's previous normal speed and jump arcs. Defaults live in
`config/server/gameplay.json`; existing course overrides keep their authored
numbers until their later retuning stage.

| Quantity | Default |
| --- | --- |
| Normal movement speed (including Hotel) | 9 m/s, the former run speed |
| Movement speed pickup multiplier (`move_speed_power_up`) | 1.5, giving 13.5 m/s; jump speed is unchanged |
| Gravity | 25 m/s² |
| Low gravity | 5 m/s² |
| Jump impulse | 12 m/s, approximately 2.88 m of rise (14.4 m with low gravity) |
| Ground acceleration | 20 m/s²; 0 to 9 m/s in 0.45 s |
| Ground deceleration | 30 m/s²; 9 m/s to rest in 0.3 s |
| Ground lateral deceleration | 40 m/s², removing velocity perpendicular to input |
| Air acceleration | 20 m/s², reduced to one quarter near the upward apex |
| Air deceleration | 30 m/s², braking release, reversal, and speed above the input target |
| Air lateral deceleration | 40 m/s², removing velocity perpendicular to input |
| Directional air speed limit | Requested movement speed, including speed pickups |
| Standing / crouched collision height | 1.8 / 0.9 m |
| Crouched ground speed | One third of normal speed |

The air limit applies to the projection of velocity onto the requested direction,
not total speed. Aligned input does not accelerate an already faster launch;
positive air deceleration slows it toward the requested speed. Sideways steering
can add velocity; with air braking disabled, coordinated air strafing can build
speed beyond the normal ground speed. Steering and braking have no separate
high-speed cutoff. Upward motion near the apex weakens air acceleration, and steep
view pitch reduces the forward air-speed target. Gravity uses a half-step on either
side of movement. Air braking changes horizontal motion only and applies the same
way before and after a portal. There is no general horizontal launch-speed clamp.

All six player control rates are m/s², independent of movement speed, crouching, and speed pickups;
those change the target speed. `ground_acceleration` builds velocity along input.
`ground_deceleration` stops released movement, brakes before reversing, and
reduces speed toward a lower target after landing, crouching, or losing a pickup.
`ground_lateral_deceleration` removes perpendicular velocity without opposing
forward acceleration. Sharp turns can therefore shed speed while suppressing
sideways slip. `air_acceleration`, `air_deceleration`, and
`air_lateral_deceleration` use the same rules in flight. Air rates may be zero:
zero stopping/lateral braking allows coasting, while opposite input can still
brake through acceleration; all three zero leave horizontal motion untouched by
input. Obby sets all three to zero. Combat knockback retains its separate decay
and participates in the air acceleration limit without passive braking storing
an opposing velocity. Ground values and the air acceleration number were retained
at the user's request; interpreting air acceleration as m/s² makes it weaker than
the previous multiplier. No takeoff or steering velocity history is tracked.

`common/src/physics/characters/player_step.rs` owns player policy over the shared
collision motor, and `player_control.rs` owns acceleration. The rendered owner
and headless owner call it through the same body-blocking planner. Reported state
includes horizontal velocity and stance; the server adopts it without player
physics or reconciliation. Crouching changes collision and hitbox size, keeps
feet fixed on the ground and the body centre fixed in the air, and refuses an
uncrouch without clearance. Angled portal transitions use the shorter hull.

The controller combines directional movement with independent ground and air
rates on our Rapier capsule motor. It deliberately departs from Source's coupled
ground friction/acceleration. The game retains its capsule collision and existing step-height limit, ladders, moving-carrier
rules, fall damage, combat knockback cap/decay, and existing portal fit/crossing
rules. Predictive funneling in `common/src/physics/portals/funnel.rs` derives
correction from available time before rim contact, including gravity and terminal
fall speed. It slides the body toward the aperture once per player step and
never changes its velocity, so a crossing leaves at the angle it went in at. It
assists airborne, approaching players
without movement input on floor, ceiling, and standable ramp portals. Capture
uses both current and predicted offsets; unrelated fly-bys and obstructed
approaches are excluded. Moving portals use their current linear velocity.
Collision remains authoritative: a late approach or abrupt carrier reversal is
not guaranteed to be captured. It does not use Portal 2's gaze or map-specific
fling helpers.

Shared portal settings live under `weapons.portals` in gameplay JSON:
`size.width` / `size.height` default to **1.4 / 2.6 m** and drive placement,
overlap checks, aperture traversal, backing exclusions, rendering, and effects.
`funnel.capture_margin` currently defaults to **0.6 m** beyond each aperture edge; zero
disables assistance. There are no separate funnel acceleration, speed, or timing
settings. The editor reads portal dimensions mechanically; its movement-preview
replacement still belongs to stage 2. The renamed `ground_lateral_deceleration`
remains independently tunable, as does `air_lateral_deceleration`.
Left/right strafe clip selection was also corrected.

Crouch presentation currently
blends camera height and model height; it does not add a new authored animation.

Reference material: Valve's [Source SDK movement implementation](https://github.com/ValveSoftware/source-sdk-2013/blob/master/src/game/shared/gamemovement.cpp)
explains the base acceleration/friction model; Portal 2 community
[movement prediction](https://github.com/p2sr/SourceAutoRecord/blob/master/src/Features/Tas/TasTools/StrafeTool.cpp)
provided the initial reference profile and the retained pitch/apex effects;
air speed limits, steering, and ground control were subsequently changed for this game.
The SDK alone is not a specification of retail Portal 2.

**Puzzle contract:** main progression uses momentum from an ordinary takeoff or
fall, portal orientation, and ordinary steering. Do not require expert air
acceleration or bunnyhopping to finish. Those techniques may reward optional
routes, shortcuts, or easter eggs. A fling aligned above the directional limit
should reach its designed landing with W released. Portal courses can disable
air braking to retain launch momentum; Obby currently disables air acceleration
as well, so its routes cannot depend on manual airborne corrections. Verify both inputs rather
than assuming they are equivalent at every view angle or launch speed.

## Continuing toward generated maps

Resume these course-design tasks after the staged movement/editor handoff above.

1. **Playtest the current course before adding complexity.** Start with the
   final approach using `cargo run --release -- --map portal_choices --checkpoint 3`;
   collect low gravity and check visibility, aiming, and unboosted movement.
   Then play from the start to assess discovery, the three ramp choices, and
   recovery after losing speed. Use graphical playback to inspect the scripted
   solution alongside free play.
2. **Turn discrepancies into reproducible cases.** Record the map/settings,
   equipment, portal placement, takeoff position, held input, and where the
   attempt fails. Compare these with the trace. A passing ideal script must
   not dismiss a player's difficulty. Keep manual confirmations separate from
   automated results when updating this handoff and [TODO.md](TODO.md).
3. **Develop tools around the unanswered design questions.** Route search,
   landing-tolerance measurements, and editor previews of ramp exits and
   mid-flight gravity changes remain useful next capabilities. The present
   editor portal preview covers floor and wall surfaces. Add guidance as
   experiments need it, while retaining the game's shared movement and portal
   simulation and client ownership of movement.
4. **Use successful experiments to inform generation.** A future generator
   needs to compose prerequisites and meaningful alternatives, emit ordinary
   editable maps, and validate completion, recovery, shortcuts, and tolerance.
   Feed actual playtest findings back into those constraints. No generation
   algorithm or automatic measure of fun has been selected or implemented.

Concrete follow-ups live in [TODO.md](TODO.md), especially the Portal Choices,
Portal Relay, graphical playback, and editor/simulation entries. Keep this guide
focused on the intent and evidence needed to continue the work.

## Play and edit the maps

For portal setup, ramp selection, and movement-pickup decisions, play
**[Portal Choices](config/server/maps/portal_choices/README.md)**:

```sh
cargo run --release -- --map portal_choices --look 90,-10
```

Its suspended `equipment_eraser` pickup removes collected boosts during a
flight while preserving the map's always-active portal gun. The adjacent
`experiment.json` also works with `--play-experiment`. The final ramp choices
face a shared destination, and a deck beneath the speed launch provides a
refill and a portal entrance for retries.

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
python3 tools/editor.py portal_choices
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
WASD moves, Ctrl crouches, Space jumps, and Q cycles weapons if needed.

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
  { "action": "move", "direction": [1, 0], "ticks": 20, "jump": true },
  { "action": "advance", "ticks": 30 },
  { "action": "check", "min": [-20, 4.3, -8], "max": [-12, 4.6, -4] }
]
```

Use `config/server/maps/portal_movement/experiment.json` for a complete executable
script; the fragment above only illustrates the action format.

| Action | Behavior |
| --- | --- |
| `move`, `direction: [x, z]`, `ticks: N` | Hold a direction for N ticks. Magnitude does not change speed; `[0, 0]` holds no horizontal input. Optional `crouch: true` holds crouch; `jump: true` attempts one jump on the first tick. |
| `advance`, `ticks: N` | Simulate N ticks with no movement input. Gravity, momentum, and actors keep advancing. |
| `aim`, `target: [x, y, z]` | Aim from the player's eye at a world point, without advancing time. |
| `portal`, `end: "a"` or `"b"` | Attempt a portal shot using the current aim and normal placement rules. |
| `check`, `min`, `max` | Test whether the living player's feet lie inside inclusive world bounds. Requires ground support by default; `grounded: false` permits an airborne waypoint. |
| `inspect` | Record state without advancing time. |
| `reset` | Recreate the server and owner, return to the scripted spawn, and clear movement, checkpoints, shots, and portals. Aim returns to +Z. |
| `fire` | Fire one ordinary projectile using the current aim. |

A direction starts in world space and turns through portals with the player's
held input. The world direction is resolved against the current aim into forward
and sideways input, so view pitch affects the forward air-speed target just as in the game.
At the end of `move`, that input is released; horizontal velocity follows the map's
air or ground braking settings. A second move can change direction in midair when
the map enables air control. Jumping uses the game's
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
position, the last position adopted by the server, vertical and horizontal velocity,
crouched stance and its blend fraction,
knockback, support, health, equipment (including `speed` and `low_gravity`), and checkpoint. These positions can differ
at lower movement report rates; the owner remains authoritative for its motion.
`active_switches` and `open_fields` contain authored names: an open field is off,
so a light bridge belonging to it cannot support the player. `fireworks_started`
records the server's celebration cue without its random presentation seed.
`item_collected` names collected items, and `equipment_erased` records the
server applying erasure from a field or pickup.

`player_step` records each tick's positions, actual combined horizontal/blast velocity,
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
cargo test --release -p cuboid-wars
```
