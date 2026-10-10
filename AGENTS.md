# Repository Guidelines

## User privacy

Do not use the user's personal name in responses or add it to documentation, examples, fixtures, comments, generated assets, or attribution. Use neutral wording and fictional sample names. Avoid recording identifying usernames or absolute home-directory paths in project files.

## Working

Work in few rounds: batch edits into one build, do small and obvious things without announcing or asking first, and ask only for decisions that are the user's to make.

## Follow-ups

Read [TODO.md](TODO.md) at the start of a task and keep it updated when discussing or completing follow-ups. Sections are Fixes (behaviour that is wrong), Enhancements (everything else, including cleanup), and Testing; no deferred or proposed categories, no approval labels. Listing an item does not authorize implementation. Remove completed items.

## Project structure

Rust workspace. The root package builds the one `cuboid-wars` executable: `src/main.rs` parses the CLI and picks the mode from one optional flag (`--host`, `--join`, `--serve`, each with an optional address; `--experiment MAP` runs the map's scripted player headless and prints its report, and a `.json` path in place of the map runs any script; `--play-experiment MAP` steps through it graphically; none is single-player). `src/experiment/` is the headless runner; `tools/map_author/README.md` covers scripts.

Entries below give a module's role and point at the comment that owns each non-obvious rule. Mechanisms, numbers, and step-by-step behaviour live in the code; read the module before changing it.

- **`common/`** — what both crates run, plus what the editor preview needs without the Bevy client.
  - `protocol.rs` — every wire message. Its top comment defines the bootstrap / state / cues / events roles and the two lanes; read it before adding a message. `network.rs` is the renet wire definition; a message's lane comes from its role.
  - `physics/` — the shared character motor (`characters/`: `step_character_movement`, support and ladder rules, the velocity components actors share, move-plan intersection, `player_control.rs` for the player's acceleration rule, `player_flight.rs` for the collision-free airborne step the editor preview runs, held to the client's full step by the client's parity tests, `falling.rs` for the one fall-damage rule), the collision world (`world/`, one `impl CollisionWorld` per concern), field passability, the blast falloff rule, the stall watchdog, and portal geometry and traversal (`portals/`). Generic collision queries stay with `CollisionWorld`; navigation policy belongs to its caller. Carriers are the one geometry that moves (`world/carrier_sync.rs`).
  - `types/` — markers, IDs, body generations, positions, movement states, map layout types, items, snapshot records, the shared clock, and the kind tables (`kind_table.rs` behind `FieldTable` and `SwitchTable`).
  - `map/` — ramp geometry, carrier poses and runs, grid ↔ carrier conversion (`MapGeometry`), grounds, rocks. Layout records are in their carrier's frame and the server fills `y`/`height` at compile time, so only grid logic needs `MapGeometry`.
  - `celestial.rs`, `math.rs`, `health.rs`, `constants.rs`, `config/` — the solar/lunar model and its tick-anchored clock, shared math and glam ↔ Rapier conversions, and the shared configuration types.
- **`map_core/`** — map source rules shared by the game and editor: `schema.rs`, `load.rs`, `diagnostics/` (the comment at `Issue` says what is an error and what a warning), `authoring/`, `transforms/`, `geometry.rs`, and `preview/` (the editor's Jump Path over `common`'s flight step; the comments at `misses` in `trajectory.rs` and `capture` in `regions.rs` own its two stand-in rules). Validation never repairs a document; the game rejects invalid sources, the editor keeps them for diagnosis and undoable repair. Documents keep their authored key order, so a save rewrites only what was edited.
- **`map_core_py/`** — thin PyO3 adapter over `map_core`, built on first use by the editor. There is no Python fallback for map rules; the comment at `grid_int` in `tools/map_editor/core.py` names the helpers that stay in Python, which `tools/tests/test_core_parity.py` holds to the Rust results.
- **`server/`** — authoritative headless server (Bevy `MinimalPlugins`), function-style domain plugins like the client; each domain's `plugin.rs` registers its systems and `resources.rs` holds its resources. `schedule.rs` defines the tick order and where deferred commands flush, so ID maps never expose unmaterialized entities to network collection.
  - `actors/`, `characters/`, `items/`, `players/`, `portals/`, `quests/`, `combat/`, `map/`, `config/` — domain systems; `combat/damage.rs` has `kill_player`/`kill_actor`, the one death sequence; `map/` compiles `map_core` documents into the runtime layout; `config/gameplay.rs` loads and projects gameplay config.
  - `projectiles/` and `missiles/` — the server keeps no bullet entities and runs no bullet or missile flight: it relays volleys, applies reported hits, and detonates a missile whose owner stops reporting.
  - `network/` — `links/` is the link layer; a closed queue or a netcode disconnect is the player leaving. Above it: ingress, authenticated routing, login, snapshots and broadcasts, server-rendered feed lines, and admin commands (`admin/command.rs` is the grammar and `/help`).
  - `actors/navigation/surface/` — the only ground navigation; there is no cell graph or second backend, and grids remain for authoring, spawning, and item placement only. Meshes bake per body and carrier in the background and the previous mesh serves meanwhile (comments on `BakedSurface`); on-demand windows beyond the authored grids are preloaded coverage, not a leash. `actors/movement/traversal.rs` executes routes through the motor (the executor keeps only the route; body state comes in and the result goes out each tick) and `movement/surface.rs` owns replanning, ladder occupancy, separation, blocking, and standoffs; each rule is a comment at its code.
  - Runs at `network.server_hz` via a manual update loop; server `Time` advances one tick per update, so timers and carriers agree and an overrun skips wall time. `NetworkConfig::validate` owns the rate constraints, run by the server after CLI overrides and by the client on the bootstrap.
- **`client/`** — Bevy renderer, input, UI. `app.rs` builds the app around the two queues; the asset root is anchored at the client crate at compile time since the executable's package is the workspace root.
  - `network/` — `link/` is the link layer with the `--lag-ms`/`--jitter`/`--drop` simulator (unreliable messages only); `bootstrap.rs` logs in and installs server state (its `install_bootstrap` comment owns configuration-dependent asset initialization); `tick.rs`/`ping.rs` hold the clock correction; `sample_buffer.rs` is the one delayed-playback buffer remote players, actors, and missiles share; `routing.rs` calls domain handlers directly.
  - `players/movement/` — the owner's simulation: `step.rs` (player policy over the motor), `jump.rs`, `momentum.rs` (`HorizontalVelocity`), `owner.rs` (`owner_tick` over the `OwnerBody` view: jump, step, crossing, outcomes, report, knockback decay), `local.rs` (runs it on the local body's components; the experiment runs it on its own fields), `planning.rs` (the shared body-blocking policy), `reports.rs`/`outcomes.rs`, and `crouch.rs` in `players/` (the presentation blend). `portals/hop.rs` is the player's crossing over the shared hop.
  - Other domains: `interpolation.rs` files blend buffered samples into gameplay components, `transform_sync.rs` files only place models. `projectiles/` owns bullet flight end to end (the comment at `step_projectile` owns the rendered/headless boundary); `missiles/` owns lock-on, guidance, and the `air_graph.rs`/`search.rs` router. `audio/`: every spatial sound plays through `LowPassAudio`, `occlusion.rs` owns the layer rules, `volume.rs` is the one writer of spatial sink volume. `ui/`: the loading screen never pauses networking or simulation; the settings menu edits live state and saves `client_local.json`; the feed only colours server-authored spans. `materials/textures.rs` owns the catalog-texture queue (its slot-release comment has the lifetime rule). `characters/model.rs` loads every GLB once (its comment says why nothing requests a sub-asset by label). `cameras/aim.rs` converges third-person shots onto the camera target; weapons consume that aim after camera sync, movement owns body facing. `players/animation.rs` and `footsteps.rs` drive from movement support and velocity, never from platform motion or knockback. `actors/aim_rig.rs` binds configured rig nodes; models face glTF +Z, aim joints use local −Z. `vfx/explosion/` is one subsystem whose marks hang under the carrier root they hit; `vfx/particles.rs` particles are cubes, so keep effects spark-sized. `fields/` draws every translucent surface (barriers, bridges, erasers, checkpoints) with one solid ↔ passable fade. `config/` holds the JSON settings and the asset set; explosion and exhaust tuning stays in `client/src/constants.rs`.
  - `client/assets/models/` — every generated GLB has its generator beside it (`modelkit/` shared); `-- --preview` renders a still.

Other paths:

- `tools/editor.py` — the PySide6 map editor (`tools/map_editor/`); takes a map name.
- `tools/mapauthor.py` — scripted map authoring, text views, flight measurement, portal shots and pair sweeps asked of the game, and experiment summaries for AI-authored courses (`tools/map_author/README.md` is the guide, `tools/map_author/` the code over the editor's Qt-free modules). An AI-authored map keeps its `build.py` beside `layout.json` as its source; a human who edits the layout in the editor updates or deletes it.
- `tools/game_review/` — the visual/performance review workflow; read its README and the platform one before client review work.
- `client/assets/` — models, textures, audio; generation scripts live beside their assets. `analyze_audio.py` measures every audio asset into `sounds/analysis.json`, which normalization applies; `--check` verifies it. Model textures add visible coarse relief or are authored for the model; plain materials for subtle paint and metal; `synth-rubber` is the approved external texture for tires and player elastomer, discuss any other third-party texture first. `ASSETS.md` is the provenance register (asset/source/license tables, `TBD` for unknowns, no local paths or narratives).
- `config/client/assets.json` — the hand-edited asset set. `config/client/client_local.json` is gitignored and carries the one version number in the project: any format change bumps `LOCAL_SETTINGS_VERSION`, and a stale file is discarded, never migrated.
- `config/server/gameplay.json` — the complete default game plus the map registry. Its defaults are the one game every map plays (grid, movement, pickups, weather, portal mode); a map's settings override only what its idea needs, and Hotel, which predates them, keeps its own. The file lists the registry, then the defaults maps override most, then the rest of the game, then `network`. `GameplayCatalog` merges each map's `settings.json` over it: objects merge key by key; scalars, arrays, and `null` replace; a tagged object replaces its default whole when its tag changes; unknown keys, global keys in a map file, and per-map `actors.<kind>.immovable` or `locomotion` are errors. The server validates once and sends a client projection in `SInit`; clients never load gameplay JSON.
- `config/server/maps/<name>/` — `settings.json` (first what it overrides in the defaults, in `gameplay.json`'s order, then the content only a map has: textures, grounds, items, quests), `layout.json` (geometry, zones, items, nested maps with embedded `nested_geometry`, plates; the root owns `switches`, `fields`, target assignments, and optional `fireworks`), optional `experiment.json` and `build.py`.
- `launch_players.sh [players] [lag_ms] [drop] [jitter]` — tiled local session (macOS). `bacon.toml` — watch jobs.

## Build, run, lint, format

Do not rebuild executable binaries for the user; they run `cargo run` themselves. The game is a prototype: fast turnaround beats certainty, and breaking something now and then is fine. Validate with `cargo check` and the tests of what you touched: work on a map runs that map's tests, not every map's. Clippy and the whole workspace run once before a commit, or when a change to shared code (movement, portals, collision, networking) could reach other maps. Running the tests takes seconds; building them is what costs, so batch edits into few builds.

Invoke commands covered by saved approval prefixes directly, without redirection, environment assignments, or wrappers; in particular `/opt/homebrew/bin/blender --background --python <script>`.

**All cargo invocations default to `--release`.** Never silently switch to debug.

After your edits, format the files you changed, and only those, so no agent reformats another's work in progress: `rustfmt` for Rust, `ruff format` for Python, `prettier --write` for hand-written JSON. Their settings live in `rustfmt.toml`, `ruff.toml` and `.prettierrc.json`; `.prettierignore` names the JSON the editor and the game write (layouts, local settings).

```bash
cargo run --release                                         # single-player
cargo run --release -- --host [0.0.0.0:8080] [--map hotel]  # accepts joiners
cargo run --release -- --join [192.168.1.100:8080] [--name "Player"] [--lag-ms 80 --drop 0.02]
cargo run --release -- --serve [0.0.0.0:8080] [--update-hz 15 --snapshot-hz 4]
cargo clippy --release --workspace --all-targets
cargo test --release --workspace
PYTHONPATH=tools QT_QPA_PLATFORM=offscreen python3 -m unittest discover -s tools/tests -p 'test_*.py'
python3 tools/editor.py hotel
```

Packages: [DEPENDENCIES.md](DEPENDENCIES.md).

## Architecture notes

The movement authority split is settled. Server-authoritative movement with reconciliation was tried and replaced for responsiveness: the client owns its movement and nothing corrects it. Do not reintroduce server mediation, reconciliation, or input replay. The [protocol header](common/src/protocol.rs) defines the ownership and consistency guarantees.

**Server is authoritative for**: spawns and relocations, actors, items, gameplay outcomes, reported bullet and missile damage, scoring, death and respawn timing, map generation (sent once in `SInit`).

**Client owns**: input, local movement, its own portal crossings, rendering, camera, UI. The server adopts every fresh, finite movement report for the current body, keeps it in its carrier frame, runs no player physics, and never repositions the living owner; the owner reports landings, crushing, void falls, and erasure reliably and applies reliable additive blast impulses itself. Respawns and relocations advance the body generation and arrive through `SPlayerRelocated`, with snapshots as a fallback.

### ECS conventions

Client movement input samples once per frame in `PreUpdate` (`movement_input_plugin`; its comment owns the order before fixed simulation). A jump is a `JumpRequest` the owner's tick consumes; its comment owns the jump buffer and coyote window, which the editor preview's late takeoff shares.

Handle discrete state at its change boundary: ingress and lifecycle handlers own network-driven state, `Added<T>`/`Changed<T>` and asset events drive the rest. Small bounded scans over the game's small collections beat indexes, caches, and synchronization invariants. Guard equal writes only when they would wake a concrete change-detection consumer, and say so in a comment. Each output component has one owner: one system computes a combined value, never two in a race. Zero-data tags use the `...Marker` suffix.

### Message dispatch

Both sides dispatch decoded wire payloads straight from ingress to one domain handler (`network/routing.rs`). Do not re-emit them as Bevy events: it was tried and only obscured the control flow, since each message has one consumer. Bootstrap precedes the gameplay app: the executable sends `CLogin`, waits for `SInit`, and builds the app from it; the rule that makes this safe without a readiness handshake is in the `protocol.rs` header. The server drops body-bound messages from a dead player; `CPing`/`CAdmin`/`CChat` and in-flight missile reports keep working through respawn.

### Protocol model

Roles and lanes are defined in the `protocol.rs` header only. Most "X changed" state belongs in `SSnapshot`; a cue is for sub-tick latency, an edge-triggered side effect, or what a snapshot cannot carry; an event only when the snapshot cannot stand in for it. Anything a one-shot carries that represents durable state must also be in the snapshot.

### Gameplay systems

Each entry gives the rules a map or config author sees, the settled decisions, and where the code is. Actor kinds and their assets are configuration data: systems select capabilities from configuration and never hard-code actor names, asset paths, or node names.

#### Death & respawn

`kill_player` (`server/src/combat/damage.rs`) is the single death entry point; its comment is authoritative for the sequence, scoring, and callers. Only missile blasts credit a killer. Deaths and departures share the `never`/`solo`/`any`/`all` trigger (`common/src/config/death.rs`) for actor and switch resets; `respawn.players` is `individual` or `group`. The comment on `players_respawn_system` owns actor-reset timing and the in-flight-shot rule.

Checkpoints are rectangular zones with a `type` (`individual`, `group_any`, `group_all`: everyone's once every logged-in player, dead ones included, has visited) and a `number` that orders the course; equal numbers are one respawn point with several rectangles, on any carriers. Number 0 is the start, required (`validate_document`), never drawn, its `type` meaningless. Progress only moves forward and survives death; disconnecting returns to the start. Login, respawn, the void rescue, and `/return` share `players/spawning.rs`. `--checkpoint <number>`, `/checkpoint [number]`, and `/return` are the debugging entry points.

#### Barriers & keys

A field (`fields` in the root layout: colour, optional `switch`, `initially_on`) is a named force field with one state; barriers (wall edges) and light bridges (floor cells) are its pieces, solid while it is on. A key lets its holder through every piece of its field and changes nothing visible. Which queries see which collision group is stated at `world_collision_groups` and `field_blocks` in `common/src/physics/world/colliders.rs`; the owner passes held keys' fields plus fields that are off, actors only the latter. Fields that are on block attack paths but not awareness, absorb bullets, and stop missiles and blasts. HUD key slots follow the fields a key is placed for.

`power_ups` defines each kind as `mode: always` (from login through death and erasure, no pickups) or `mode: pickup` with `duration_secs` (`null` lasts until death or erasure). `placed_items.respawn_secs` is a sparse per-type table: positive respawns, zero allows immediate recollection, omitted or `null` never respawns; `placed_items: null` makes every placed pickup one-time. A placed item needs no floor, only a cell outside every ramp footprint; the pickup rule is the comment at `character_overlaps_item`, and an item that would change nothing for its player, a reset timer or closed portals included, stays in the world (`pickup_has_effect`).

#### Equipment erasers

Per-level `erasers` grid edges. The owning client reports `EraseEquipment` while in swept contact; the server applies it after item collection, so erasure wins a same-tick pickup, and keeps no occupancy state. Actors, projectiles, and missiles pass through; portal shots do not. The `equipment_eraser` item floats without a floor and reappears under `placed_items.respawn_secs`. Erasure, by an edge or the item, clears collected power-ups and missile ammo and closes the player's portals (`erased_portals_system`); every always-held gun and ability stays, and so do keys, health, score, and quests. Erasers, pickups, and ladders are hand-tested volumes, not Rapier sensors: sensors would need the collision pipeline and still the swept test for fast crossings and moving fields.

#### Light bridges

Barriers laid flat: solid and lit while on, a faint ghost while off. Authored per cell and merged largest-rectangle-first at compile time (`server/src/map/bridges.rs` says why). Bridges set no floor flags, so item and spawn cells never see them; only the panes fade, the slab keeps its footprint. Ground actor meshes include powered slabs and rebuild when fields change.

#### Switches and pressure plates

`layout.json::switches` defines named on/off controls; pressure plates are the physical input, any number per switch. Fields, actor zones, and nested maps carry `initially_on` (default `true`) and may name a `switch`, which flips that state while active (solid, spawning, running, or heading for end 2). `fireworks` names its switch alone. A target without a switch, or on a switch no plate operates, keeps its initial state; the second is a warning, so plates and targets may be authored in any order. A switch configures `activation` (`momentary`, `toggle`, `auto`: toggle with one logged-in player, momentary otherwise; `latch`: on at the first press until a death reset), `reset_on_player_death` (`never`, `solo`, `any`, `all`), and `held` (`any`: one occupied plate; `everyone`: every living player on a plate, or every plate held when players outnumber them). `server/src/map/switches.rs` owns activation, resets, and the `SwitchState` projection (every change goes through `Switch::set_active`); `pressure_plates/` owns occupancy. The plate chain runs after the tick advance so a flip stamps the tick the carriers advance to. The fireworks switch's plates are inert and hidden while a fireworks quest is locked.

A plate takes one size in metres where the floor around its cell has room and shrinks only to keep clear of walls, fields, ladders, holes, ramps, and other plates (`pressure_plate_sides` in `map_core/src/geometry.rs`, which the editor draws with); its square is what a player stands on to press it. Plate collision is one fixed box per plate that both motors use, ignoring the model's clipped corners and tread animation. Tread colours follow the first field the switch drives unless the switch sets `color`.

#### Quests

The map's `quests` load into the immutable `QuestCatalog`; `QuestBoard` is session state. Every `SQuestUpdates` entry repeats the quest's complete definition and state, so the client needs no catalog, and merging is monotonic; `SSnapshot.quests` self-heals group state. A quest has a `kind`, a `scope` (`individual`, `shared`: one pooled counter, `everyone`: own progress, completing for the group once every active player reaches the threshold), points, and optional `requires`, which hides it until the named group quest completes. Kinds advanced by a world event (`fireworks`) must be `shared`. `/quest` lists and completes quests by fiat.

#### Character movement

`step_character_movement` takes a `CharacterStep` with an `intent_velocity` (read only by ladder decisions), the `velocity` the body moves at, and a `displacement` that is no velocity, so a shove or launch never starts a climb. `CharacterStart::probe` is the one start-of-tick judgement of carry, support, and ladder (its comment says why it probes where the ride puts the body); the player step reads it and hands it to `step_character_movement_from`. The motor integrates gravity in half-steps for actors and players alike. `step_player_movement` (`client/src/players/movement/step.rs`) and `step_actor_movement` (`server/src/actors/movement/step.rs`) apply policy over it.

The client's fixed step is the owner's simulation, not a prediction: nothing corrects it, a dead local body stops stepping, and only a new body generation moves it. The owner reports its full state at `update_hz` in the frame of the carrier it rode and its outcomes as `CMoveOutcome` events; the server keeps the newest report whole, resolves it against the carrier pose, and relays it. Server player entities carry only position, facing, and health. Every server placement goes through `place_player_body`, and `PlayerMap` remembers retired generations so late snapshots and cues cannot revive them.

Players have persistent horizontal velocity and view-relative input at one speed; portal exits use the same air rules as any flight. `accelerate_player` owns independent ground/air acceleration and braking. The model follows Valve's [Source SDK movement](https://github.com/ValveSoftware/source-sdk-2013/blob/master/src/game/shared/gamemovement.cpp), profiled from the Portal 2 community's [strafe prediction](https://github.com/p2sr/SourceAutoRecord/blob/master/src/Features/Tas/TasTools/StrafeTool.cpp) at 0.025 m per Source unit, with ground and air rates separated and its own speed limits; the SDK is not a specification of it. `PlayerStance` is the hull alone and rides reports; `CrouchBlend` is each client's own eye and pose blend. A move another body rejects keeps the velocity along that body and loses the velocity into it, like a wall contact.

Observers play remote bodies through the shared `SampleBuffer`, `interpolation.buffer_intervals` behind the newest sample; riders report carrier-local positions. Clients treat interpolated bodies as stationary obstacles. Characters are not Rapier colliders: movement and projectile scans iterate the small character lists with swept capsule tests (the comment at `character_paths_intersect` explains the query). Registering characters in the broad phase was weighed and rejected at these counts, as were Parry contact queries while every shape is an upright capsule. Fall damage converts landing speed to an equivalent normal-gravity drop (`landing_damage`), which the editor preview shares.

#### Bullets

Every client simulates visible bullet flight, ricochets, and portal crossings; only the shooter reports character hits, reliably, while volleys ride unreliable cues. `MuzzleCheck::Enforced` decides the volley once on the shooter. Delivery and lifecycle rules are in `protocol.rs`.

#### Missiles

Ammo comes from `missile_pack` items capped by `missiles.max_missiles`; there is no cooldown. All launch feedback waits for `SMissileLaunch`, so a rejected shot never orphans a cue. The shooter owns the whole flight: lock-on, guidance over `AirGraph` routes under a shared per-tick search budget (accounting in `missiles/search.rs`), the fuse, and reliable reports of detonation and victims. The server applies its own falloff (`blast_hit`), checks victim generations, and detonates a missile whose owner stops reporting. Missiles do not traverse portals; a stalled one self-detonates through `ProgressWatchdog`.

#### Portals

Q cycles owned weapons; `WeaponMode` is kept inside the loadout every frame. Login hands each player a slot: `both` mode gives every slot a pair, `auto` mode shares one between adjacent slots, a lone player holding both ends until a partner joins. The firing client validates placement with `compute_portal_placement` (solid backing on one carrier, clear front, per-face `portalable` permissions from the map's texture catalog, a Portal-2-style nudge when the fit fails; backing may span adjoining coplanar cells or stacked wall sections) and sends a carrier-local placement; the server trusts the geometry and checks generation, assignment, equipment, cooldown, and overlaps, keeping no `PortalSet`. A portal's wire value is in its carrier's frame and stays constant while it moves. Overlaps are compared at the current tick only; gates on different carriers may coincide later, harmlessly.

Traversal is true pass-through (`common/src/physics/portals/traversal.rs`): in a linked aperture's front corridor the motor excludes the backing colliders, whatever lies wholly behind the plane as deep as a body sinks before it crosses (the floor under a ramp compiles apart from the floor beyond its toe for this); actors never fall through, remote players use reported crossing boundaries. The carried-aperture rule is at the top of `step_character_movement`; `rider_carry` says when a body in a carried corridor still rides. A crossing preserves the velocity relative to the entry's carrier as velocity relative to the exit's (`character_hop`). Only the owning client crosses a player (`client/src/portals/hop.rs`) and reports at once; the repeated crossing marker lets observers cut without acknowledgments. The aperture frame derives from the surface normal alone, so ramps work unchanged. The funnel (`PortalSet::funnel_correction`) moves a body onto the aperture and never changes its velocity, so a body leaves at the angle it entered; its catch is judged where the flight would come down, within a margin that grows with the time still to fall (`PortalFunnelConfig::margin_at`). Aperture size and margins are under `weapons.portals`; `PortalFrame.size` keeps placement, traversal, and rendering consistent.

Portal surfaces are off-axis render targets rooted at the main camera, admitted largest on screen first under `preferences.portal_view_budget` (0 disables see-through); portal cameras render linear HDR so the presenting camera tonemaps once. A body straddling a gate is drawn on both sides through a hidden twin and `PortalClipMaterial`.

#### Ramps

`ramps` records: `{lower_level, levels, cols, rows, direction, shape}` plus face materials, `solid` or `plank`. Nothing reads the slope off the footprint: an over-steep ramp is valid and merely unclimbable. `common/src/map/ramps.rs` derives the prism collider and mesh share. A ramp needs no floor: canonicalization opens the floors and terrain it passes (`terrain_excluded_cells` says why solid and plank differ) and slabs end flush at its landings. Two ramps may share cells only where one ends on the level the other starts; other records stay off its footprint. A carried ramp neither pushes nor crushes.

#### Ladders

`ladders` records anchor a one-sided, freestanding ladder on a grid edge; nothing inspects surrounding geometry. Climbing derives per tick from position and intent against the front-only `LadderVolume`, a plain AABB; `clamp_move_at_ladder_plane` owns the fence rule. In the front volume a body moves at `move_speed_ladder` times the move speed, and a jump lets go in the held direction instead of rising, forward through the rungs (the `on_ladder` comment in the client's `step.rs`, `player_jump` in `jump.rs`). Actors opt in per kind (`can_use_ladders`, all shipped kinds `false`); routes carry explicit mount, climb, and exit actions, one actor per ladder.

#### Carriers

A carrier is a rigid group of records sliding between two endpoints; every carrier is a nested map. `motion` is `cycle` (an optional switch pauses it) or `follow_switch` (end 2 while on). `common/src/map/carriers.rs` owns the tick-anchored run; nothing per free carrier rides the wire, since both sides evaluate the trajectory from the tick. The world is carrier 0; layout records name their carrier and are in its frame. `carriers_advance_system` runs right before character movement on both sides; `CollisionWorld::set_carrier_poses` is the one place a collider moves. Riding is inside the motor (`supporting_carrier` owns the rule); a departing player keeps the ride in `HorizontalVelocity`, actors keep none. Nothing keeps a path clear: `push_character_from_carriers` resolves side contacts and a remaining overlap is a fatal crush. Carriers are not Rapier kinematic bodies: those add an integration step without replacing the riding, pushing, and crushing policies.

#### Nested maps

`nested_geometry` holds named definitions; a `nested_maps` entry `{map, level, from, to, to_level, travel_secs, pause_secs, phase_secs, from_nudge, to_nudge}` places one and slides it, `from == to` on one storey being a room. Nesting is recursive; each compiles as a carrier in its own frame. `load.rs` rejects missing references and cycles; unplaced definitions contribute nothing; kinds resolve against the root's catalogs. Nested geometry may not define `switches`, `fields`, `fireworks`, or `nested_geometry`. Grid data is per carrier; random items use the root grid. Nothing checks overlap between a nested map and its parent. The per-grid level limit is at `validate_map`.

`settings.json::grounds` extends solid terrain around the root geometry at `level`; walking off its far edge is a void fall and rescue. Only the root carrier's geometry cuts the landscape (`server/src/map/definition/grounds.rs`); terrain and decorations are the same triangles for collision and rendering. Per-level `terrain` entries are floor slabs and need no duplicate `floors`. Pebbles are never solid; trees, stones, and boulders are.

#### Weather & celestial lighting

One procedural HDR sky. `celestial` (a `gameplay.json` default a map may override) gives latitude, season, north yaw, start time, and moon phase; north is +Z, east +X. `CelestialClockAnchor` rides `SInit` and every snapshot; clients extrapolate against `ServerTick`. `/time` and `/moon` control it. `SkyState`, derived once per frame, drives the sky material, both lights, ambient, grading, and fog; `map/clouds.rs` mirrors the cloud layer so a cloud over the sun dims the light; the comment on `SkyProbe` says why it crossfades; the buffer-packing constraint is beside `ProceduralSkyMaterial`. Weather is independent: a map's mode seeds `weather_system`, `cloud_cover` and `raining` ride each snapshot. `/weather` reports or holds it.

#### Actor lifecycle

`actors_removal_system` routes deaths and crushes through `kill_actor`; lethal landings credit no one, and a ground actor that falls out of the world despawns silently. `actors_respawn_system` refills each vacated slot after the zone's `respawn_secs` (`null` never). `ActorSpawnZone::is_enabled` is the one gate; the switch never touches a countdown. `until_checkpoint` spawns only while the live progress is short of it; `on_checkpoint` is `stop` or `destroy`. `respawn.actors.on_player_death` selects the shared trigger and `scope` (`dead` or `all`). Replacements reserve ids and spots in `PendingActorSpawns` and materialize after `beam_in_secs`; the ghost's fade is a pure function of the shared tick.

Spawn-zone `count` is a nondecreasing list: targets for one, two, three, and four-or-more logged-in players (dead ones count; an empty server uses the first). The fill tops each zone up every tick, so a join spawns only new slots and a rejoin never revives a permanent kill. A fill with no free spot retries every tick and warns once: a crowded zone is a map design problem.

#### Actor AI

All movable ground actors use surface navigation; flying actors keep incremental 3D navigation without gravity; anchored actors only target. Goal decisions run at 10 Hz, everything else every tick. The movement set runs anchored placement, then ground, then flying, each sweeping against what came before. `roam_distance` per zone extends roaming from the spawn volume; pursuit and evasion may leave it, and actors return to the actual volume before roaming again (`behavior/home.rs`). Missing support and stalls lead to waiting or replanning, never random escapes. Threats are acquired by line of sight and remembered for `threat_memory_secs` in the carrier frame they were seen in; `SurfaceNavigation::pursuit_goal` says why pursuit aims at the nearest occupiable point. An unreachable threat sends an armed map's actors to cover; when `players_can_be_armed` is false, unreachable players are ignored. `/peace [on|off]` and `--peace` make actors forget and stop attacking; `--god` sets `Invincibility`.

### Conventions

- Pre-release: JSON configs, map files, and the wire protocol are unversioned and breaking changes are allowed. Update every producer, consumer, fixture, and checked-in config together; no version fields, compatibility branches, or migrations. The one exception is `client_local.json` above.
- JSON uses `[]`, `{}`, and `null` for empty lists, dictionaries, and absent values; an empty object does not disable a block. `grounds`, `random_items`, `placed_items`, and root `fireworks` require an object or explicit `null`. Required nullable fields use `deserialize_required_option` (its comment explains the Serde constraint). Nested-map `to_level` omitted or `null` inherits `level`. Collection fields never use `null`.
- Entity IDs are newtypes (`PlayerId(u32)`, `ActorId`, `ItemId`, `MissileId`, `FieldId(u16)`); `PlayerMap`/`ActorMap`/`ItemMap`/`MissileMap` map them to entities on both sides.
- Shared player/actor behaviour belongs in `characters`. Portal systems own portal behaviour; movement and gameplay effects must run without portals.
- Crossbeam channels carry decoded messages; one system per frame hands the batch to renet, so gameplay code never touches the transport and broadcasts do not know which players are remote. The host's client holds the other ends of its embedded server's queues. A closed queue is the disconnect. There is no async runtime.
- Coordinates: Bevy Y-up, metres. Wire format: `bincode` 2.
- Network reviews follow the **Self-repairing state** contract in the protocol header; temporary inconsistencies alone are accepted behaviour.
- `score` persists across deaths; do not confuse with `health`. Server "dead" (`PlayerInfo::is_dead()`, no entity) and client `LocalPlayerInfo.is_dead` are separate flags in separate crates; do not unify them.
- Layout records are compiler input: the renderer builds each record's own mesh and the collision world its collider. Whatever else needs to know where the structure is asks `CollisionWorld` (rays, casts, `structural_solids` for the convex faces in a carrier's frame) and never reads the wall, floor, and ramp records, which miss trim strips and any geometry a record does not name.
- Mesh UVs come from the record's position in its carrier's frame, `(carrier_center + rotation * local_pos) · uv_axis / tile_size`, never from the mesh's local position, so a carried texture never swims.

## Map editor (`tools/editor.py`)

Requires Rust, Python 3.10+, PySide6; only maps registered in `gameplay.json` open. Python keeps widgets, selection, document transactions, autosave, and undo; geometry, normalization, validation, nesting, and transforms are adapters to `map_core`, so map rules go there.

The canvas IS the UI: no coordinate readouts, row/col numbers, or status-bar grid info. Feedback is a temporary canvas notice, tool properties sit in the toolbar, and Tools and Properties stay visible.

- The editor never writes `settings.json`; it reads the map's effective settings (`load_map_settings`), `assets.json` (wall-light kinds), and `symbols/items.json`, and watches the files; texture images are neither read nor watched.
- `MapDocument` owns transactions and undo across the whole parent document, nested geometry included. Widget-free operations (`editing.py`, `erasing.py`, `regions.py`, `transforms.py`, `nesting.py`) import no Qt and work in grid units; `viewport.py` converts once.
- Loading keeps invalid records; `repairs.py` proposes undoable repairs; warnings never block an edit, save, or load.
- One selection (`selection.py`, published by `selection_state.py`); `interaction.py` owns the one active gesture and resolves targets identically for both buttons. Tool, level, geometry, scope, or level-count changes clear it.
- Properties has no Apply step; `SelectionProperties.commit` owns commits, failing drafts, and undo merging. Shortcuts are scoped to the canvas.
- Placement never deletes a ladder or moves a nested-map end. Moving or deleting a floor or wall takes what it held (`with_supported`); validation refuses removing any other unselected dependent. In-use fields and switches cannot be deleted; renames update references.
- Checkpoint numbers may repeat; nothing renumbers silently. Number 0 is the start, always `individual`, seeded in a new map; Edit Checkpoints renumbers as one undo entry with zones' `until_checkpoint` following.
- Spawn zones get no support or capacity checks. Edit Levels and Resize Map stage until OK and apply as one undo entry.
- Measure tools: Jump Path (`jump_path.py` model, `jump_path_overlay.py`, `jump_path_painting.py`, `jump_settings.py`, `portal_surfaces.py`, `floor_footprints.py`) is one `map_core::preview` call over the game's own step, funnel, and hop, from a takeoff point on a tile edge, per power-up selection, with optional portals; Python keeps picking, surface permissions, floor support, and painting. Run Time is `run_time.py`.

## Adding a texture

Texture sets are freepbr.com UE packs. Use a distinct model texture only when it shows at gameplay distance. Each generated model has a `.json` beside its GLB for material tuning (`client/assets/models/MATERIALS.md`).

1. Keep only used maps in `client/assets/textures/<name>-ue/`, resized to 2048².
2. `combine_metallic_roughness.sh <dir>/<name>_roughness.png <dir>/<name>_metallic.png` builds the packed map (ImageMagick); `multiply_intensity.sh` retunes it.
3. Add `materials.<name>` to `config/client/assets.json` with `base_color` (`_albedo`), `normal` (name must carry `-normal-dx` or `-normal-gl`), `occlusion` (a neutral white image when the pack has none), `metallic_roughness`, `tile_size` in metres, `repeat` and `linear_data_textures` true.
4. Alias it in each map's `settings.json::textures` with an explicit `portalable`; no map may define `terrain`. Ladder, rock, and terrain bindings in `assets.json` name texture sets directly; plates use their GLB's materials.
5. Compare catalog paths with the directory listing character by character (macOS is case-insensitive). `cargo test --release -p client assets` validates the entry, not the files.

## Coding style

- Rust edition 2024; `cargo fmt` per `rustfmt.toml`. Workspace dependencies in the root `Cargo.toml`; Bevy defaults are disabled at workspace level and enabled by the client.
- Lints: `unsafe_code = "forbid"`, `unwrap_used = "warn"` (prefer `expect("what is wrong")`), `todo = "warn"`.
- `constants.rs` holds appearance, gameplay, and performance tuning, grouped by a shared prefix; implementation details stay beside their code.
- Nested `use` trees, one per crate root; no `use ... as` unless a collision leaves no alternative. No `Arc` for convenience.
- `assert!` family only; `debug_assert!` is a no-op in the builds we run.
- `mod.rs` holds only `mod` declarations and `pub use`; never pair `<name>.rs` with a `<name>/` directory.
- No comments by default; only a non-obvious WHY, evergreen. When a comment owns a rule, point to it from here instead of restating it.

## Testing

Writing tests is what costs, so the default is no new test. Write one for an algorithm that is easy to get wrong, or for a bug that came back; never for cosmetic tuning, getters, duplicated constants, or restatements, and check everything else by playing. A test that breaks under an intended change is fixed if that is quick, deleted if not; one that breaks without an intended change caught a bug.

Fixtures: a shared fixture may load a shipped configuration as the base of a whole-schema value, and every test pins the values it depends on. Whatever can be small is test-owned; do not copy a shipped file into a fixture or add production defaults for tests.

Layout: tests live in `tests/` beside the module, one file per source file that has tests worth keeping (a source file with none has no test file), declared at the end of the source file with `#[cfg(test)] #[path = "tests/x.rs"] mod tests;` so `super` is the module under test; directory modules declare theirs from `mod.rs`. Shared fixtures keep their names through `#[path]` (`test_geometry`, `test_fixtures`, `config::fixtures`). No inline `mod tests`, no crate-level integration test directories. Name tests after what they assert. The editor's `unittest` suite is in `tools/tests/`.

## Documentation

- `README.md` is for players: what the game is, how to run it, controls. No rules, config paths, editor workflows, or a line per feature; it is not a changelog.
- `PLAN.md` is the high-level game plan, for the human and the AI alike: the goal, priorities, what is fun, how we work, design principles, and the roadmap, in plain language. Read it before map or gameplay design work. No mechanics, tool reference, or follow-up lists.
- `AGENTS.md` is loaded every session, so it holds only what the code cannot supply: settled decisions and rejected alternatives, invariants spanning crates, conventions, workflows, the config semantics a map author sees, and where things live. Mechanisms, numbers, and step-by-step behaviour belong in code comments. Write at the depth of the sibling entries.
- No document hands the user work: no pending playtests, reviews, or other tasks for them in `PLAN.md`, `TODO.md`, READMEs, or here. Say what needs them in the conversation.

## Commits & pull requests

- Whenever the repository has uncommitted changes, end with a suggested commit message covering the entire outstanding set, including earlier turns and user edits; never assume earlier work was committed.
- Short, imperative summaries. PRs describe client/server impact, include repro steps or screenshots for client-facing changes, and call out protocol or asset changes.

## When in doubt, read

- Protocol: the header of `common/src/protocol.rs`. Collision groups: `common/src/physics/world/colliders.rs`.
- Death and respawn: `kill_player`, `players_respawn_system`, `client/src/network/players/sync.rs`.
- Map data: `common/src/types/` and `config/server/maps/hotel/layout.json`. Tuning: `config/server/gameplay.json` and each map's `settings.json`.
- Missiles: `client/src/missiles/guidance.rs` and `air_graph.rs`. Admin commands: `server/src/network/admin/command.rs` (`HELP_TEXT`).

## Security & assets

- Development and LAN play assume cooperative clients. Rate limits, ingress budgets, bounded queues, flood protection, and admin authorization are deferred until a public server.
- `client/assets/` are not open source; replace before publishing a fork.
- netcode runs unsecure (zero key): anyone who can reach the port can join.
