# Repository Guidelines

## Project structure

Rust workspace with three crates:

- **`common/`** — shared between client and server.
  - `protocol.rs` — all `ClientMessage` / `ServerMessage` variants and wire structs. Read the top-of-file doc comment before adding a new message — it lays out the bootstrap / state / cues / events roles and the two lanes that decide where new messages go.
  - `network.rs` — the two QUIC lanes and their carriers; the unreliable lane never drops and keeps no state. The header comment is the authority. Which lane a message rides is decided by its role in `protocol.rs` (`Lane`, `lane()`).
  - `physics/` — shared player/projectile movement, collision world (incl. per-kind barrier collision groups, plus the non-solid ladder climb volumes in `world/ladders.rs`), barrier passability, spawn validation helpers, missile lock-on acquisition (`lock.rs`, used by the client crosshair and server fire validation), and portal geometry + traversal (`portals/` — aperture frames, placement/overlap validation, character/projectile hops, and the `PortalSet` both sides rebuild from the replicated portal list).
  - `types/` — shared markers, IDs, positions, movement states, map layout types (`types/map_layout.rs`), items/power-ups, snapshots, and the kind tables. `types/kind_table.rs` holds the generic `KindTable<K: KindId>` that both `BarrierKindTable` and `BridgeKindTable` alias, so one implementation carries the id↔string mapping, the max-kinds check, and the error wording for both catalogs. `types/plates.rs` holds `PlateState`, the single value naming everything the pressure plates currently hold.
  - `map/` — shared map behaviour: level classification + ramp surfaces (`levels.rs`), grid↔world conversion (`geometry.rs`, `MapGeometry`).
  - `health.rs`, `constants.rs` — the `Health` type with its operations, and gameplay constants.
- **`server/`** — authoritative headless server (Bevy `MinimalPlugins`).
  - `app.rs` builds the ECS app and installs function-style domain plugins, matching the client pattern; each domain's `plugin.rs` owns its system registration. `schedule.rs` defines their cross-domain tick order (`Prepare` → ingress → behaviour/movement → combat damage/removal/explosions → lifecycle/maintenance → snapshot). Deferred commands are flushed after preparation, ingress, and combat, and again immediately before snapshots, so ID maps never expose unmaterialized entities to network collection.
  - `actors/`, `characters/`, `items/`, `players/`, `portals/`, `projectiles/`, `quests/` — server-side domain systems. Each domain keeps its Bevy resources in its own `resources.rs` (`players/resources.rs` separates `PlayerInfo` into connection, session, and per-life state inside `PlayerMap`, while `players/falling.rs` owns `PlayerFallState`; `quests/catalog.rs` holds immutable definitions/indexes, `quests/resources.rs` the mutable session-wide `QuestBoard`, and `quests/progress.rs` the `record_event` entry point). Actor AI uses separate contact/beam controllers over `Roam`/`Engage`/`Evade`/`ReturnHome` modes; navigation precomputes graph-based home/roam territories from spawn zones, and every mode uses the same waypoint route follower.
  - `network/` — the whole networking concern: async QUIC transport (`transport.rs` — accepts connections and drives the lanes per client), Bevy ingress and authenticated client-message routing (`incoming.rs`/`routing.rs`/`handlers.rs`), login, snapshot broadcast (`snapshot.rs`/`broadcast.rs`), server-rendered feed lines (`feed.rs` — wording, styled spans, broadcast/private audience), and admin commands (`admin/handler.rs` owns authorization and replies, `admin/command.rs` the grammar and `/help`, and `admin/execute.rs` world mutation; world-affecting commands announce to everyone, the rest reply to the issuer).
  - `missiles/` — the seeking-missile weapon: fire validation + launch (`spawn.rs`), guidance (`guidance.rs` — lead pursuit, serpentine weave, proximity fuse, obstacle avoidance), movement/detonation (`movement.rs`), and `air_graph.rs` — a full-3D BFS over the map's airspace (per-cell-per-level air volumes + a sky layer), deliberately separate from the actors' floor-walking `NavGraph`.
  - `combat/` — damage application + `kill_player`/`kill_actor` (`damage.rs`, the one-stop death sequence) and blast resolution (`explosions.rs`, with `PendingExplosions` in `resources.rs`; missile blasts carry shooter kill credit).
  - `map/` — converts map definitions into runtime layout: cells/edges, floors, walls, ramps, barriers, light bridges (`bridges.rs` merges same-kind cells into maximal rectangles), lights, masks, segments; the runtime map model lives in `map/resources.rs`. Also the weather and day/night schedulers (`weather.rs` and `light_cycle.rs` — driven by `cycles.weather`/`cycles.lighting` and each map's `weather`/`lighting` mode, broadcast as `SSnapshot.rain_intensity`/`lighting`, overridable via `/weather` and `/light`).
  - `watchdog.rs` — `ProgressWatchdog`, the one stall detector shared by actors (shake loose to a neighboring cell) and missiles (self-detonate).
  - `config/` — server config split by concern: QUIC setup (`network.rs`), gameplay loading/projection (`gameplay.rs`), map registry (`maps.rs`), and focused actor, combat, cycle, item, missile, quest, scoring, feed, and validation modules.
  - Runs at 30 Hz via a manual `app.update()` loop.
- **`client/`** — Bevy renderer, input, UI.
  - `app.rs` builds the Bevy app, loads and validates client config, installs shared resources, and registers domain plugins. `main.rs` owns CLI parsing, QUIC/Tokio setup, login, and process lifecycle.
  - `network/` — `transport.rs` drives the lanes; `impairment.rs` holds the `--lag-ms` FIFO delay stage and the `--drop` loss simulator (unreliable messages only, discarded before delivery); `resources.rs` holds `LastSnapshotSeq`, the one ordering guard, on snapshots; `io.rs` owns the receive loop, the ping cadence, and `apply_pong`; `routing.rs` unwraps `ServerMessage` envelopes and calls focused domain handlers directly (`players/`, `actors/`, `items/`, `missiles/`, `portals/`), all of which read the one `ServerMessageContext` in `context.rs`; `bootstrap.rs`, `quests.rs`, `snapshot.rs`, and `presentation.rs` own the remaining message handling.
  - `players/`, `actors/`, `characters/`, `items/`, `projectiles/`, `missiles/`, `bridges/` — client-side domain systems (`transform_sync.rs` files hold the per-frame interpolation systems; the shared character animation observer lives in `characters/animation.rs`). `missiles/` holds the procedural missile mesh, dead-reckoning movement, and the crosshair lock-on detector (`lock_on.rs`). `bridges/` holds the light bridge slabs: one material per kind (`assets.rs`), one entity per replicated rectangle (`spawn.rs`), and the alpha ease between the unpowered ghost and the powered surface (`fade.rs`).
  - `input/`, `cameras/`, `ui/`, `vfx/` — client-only interaction, rendering support, presentation. The explosion effect is one subsystem in `vfx/explosion/` (assets, spawn, animation, scorch, shards, smoke); `vfx/` also holds the zapper laser beam (`laser.rs`), rain (`rain.rs`), missile exhaust (`exhaust.rs`), and the shared GPU particle clouds (`particles.rs` — every particle in the game is a cube; keep new effects spark-sized or they read as floating boxes), and the seeded client-side firework show (`firework.rs`, played on `SFirework`; a show still playing ignores further ones, since only the client knows a show's length). The Esc-toggled settings overlay (`ui/settings_menu/`) edits live state — window mode/vsync on the `Window`, the rest on `ClientSettings`/`GlobalVolume` — using Bevy's headless `bevy::ui_widgets` behaviors under game-styled wrappers. Panel edits save to `config/client/client_local.json` on close; Cmd/Ctrl+F and F11 fullscreen changes save there immediately. The chat + admin console (Enter or `/`, ↑/↓ history) lives in `ui/console.rs`; its editor emits a typed submission and a separate adapter sends `CChat` or `CAdmin`. The message feed (`ui/message_feed.rs`) only maps server-authored `SFeed` spans to client colors. The feed and typed/config-driven HUD banner (`ui/hud_banner.rs`) are timed-line stacks (`ui/timed_lines.rs`: rows that live, fade, and go); the console prompt is the last row of the feed column.
  - `map/` — client map rendering and geometry spawning; procedural grass (incl. burn response) in `map/grass/`, skybox in `map/skybox.rs`.
  - `config/` — JSON-backed settings split by concern (`settings.rs` root + `audio`/`camera`/`hud`/`rendering`/`vfx`) plus the asset set (`assets.rs`).

Other notable paths:

- `tools/editor.py` — launcher for the PySide6 map editor (code lives in `tools/map_editor/`); takes a map name and edits `config/server/maps/<name>.json`.
- `client/assets/` — 3D models, textures, audio.
- `config/client/assets.json` — hand-edited asset set (materials, material rules, models, sounds, barrier and bridge kind colours).
- `config/client/client_local.json` — local values from the settings menu and fullscreen shortcuts. Gitignored, so `git pull` cannot update it: unlike every other JSON it carries a version, and any format change must bump `LOCAL_SETTINGS_VERSION` (`client/src/config/local.rs`) — a stale version is discarded and rewritten, never migrated.
- `config/server/gameplay.json` — the sole gameplay configuration. It holds player and per-kind actor bodies; shared projectile, missile-lock, and portal tuning; server-only actor behaviour, missile guidance, combat, scoring, power-ups, feeds, and weather/lighting cycles; and the named-map registry. `ServerGameplayConfig` nests exactly like the file (`player`, `actors.{settings,kinds}`, `weapons.{projectiles,missiles,portals}`, `items.power_ups`, `combat`, `scoring`, `cycles.{weather,lighting}`, `feed`, `maps`), so a validation error's path is the JSON path. Each map owns its complete `movement` block (gravity, player/actor/projectile/missile speeds, ladder climb ratio, knockback), weapon availability, required nullable `barrier_kinds`, optional `random_items`, placed-item respawn times in `placed_items`, quests, skybox, weather, and lighting. A quest's points live beside that quest, and quest ids need only be unique within their map. `default_map` selects the map unless `--map <name>` overrides it. The server validates everything once and sends a client-only gameplay projection plus the selected map through `SInit`; clients do not load gameplay JSON. Actor movement tuning is resolved once into each server actor's component rather than looked up by kind during movement.
- `config/server/maps/` — one map JSON per named map (geometry, zones, placed `items`, and `pressure_plates` — each with a `type`: `barrier` plus the `kind` it opens, or `firework`; per-map tuning and kind catalogs live in the `maps` registry in `gameplay.json`).
- `cert.pem` / `key.pem` — local-dev TLS for QUIC (not production-safe).
- `launch_clients.sh` — spawns N tiled windowed clients for local multiplayer testing (`./launch_clients.sh [num_clients] [lag_ms] [drop]`, macOS).
- `bacon.toml` — `bacon` job definitions; use `bacon clippy`, `bacon test`, etc. as the watch loop.

## Build, run, lint, format

**All cargo invocations in this repo default to `--release`.** Debug builds pull in too much and we don't run them — never silently switch to debug.

```bash
cargo build --release
cargo check --release
cargo run --release --bin server                            # bind 127.0.0.1:8080, loads default_map
cargo run --release --bin server -- --bind 0.0.0.0:8080
cargo run --release --bin server -- --map hotel             # override default_map
cargo run --release --bin client                            # connects to 127.0.0.1:8080
cargo run --release --bin client -- --server 192.168.1.100:8080 --name "Player"
cargo clippy --release --workspace --all-targets
cargo fmt
cargo test --release --workspace
PYTHONPATH=tools QT_QPA_PLATFORM=offscreen python3 -m unittest discover -s tools/tests -p 'test_*.py'
python3 tools/editor.py hotel                               # edits config/server/maps/hotel.json
```

## Architecture notes

**Server is authoritative for**: player and actor positions, all collisions, items, actor behaviour, projectile resolution, scoring, death/respawn timing, map generation (sent once on connect via `SInit`).

**Client owns**: input, local movement prediction, rendering, camera, UI, the death overlay.

### ECS system ownership and cadence

Prefer handling discrete state at its change boundary when that keeps the code simple: ingress/lifecycle handlers own network-driven entity state, mode transitions own their presentation state, queries filtered with `Added<T>` handle entities created after initial setup, `Changed<T>` propagates component changes, and asset events drive post-load asset work. Small bounded scans are fine—especially for the game's small player, actor, and item collections—and are better than duplicated indexes, caches, or synchronization invariants. Guard equal writes only when they would wake a concrete change-detection consumer or renderer/UI propagation; a nearby comment should name that consequence.

Each output component has one semantic owner. If visibility or another final value combines multiple inputs, one system computes the complete value; independent systems must not overwrite one another in a scheduled race. Zero-data ECS tags use the `...Marker` suffix. Before adding maintenance bookkeeping, compare its complexity and consistency cost with the bounded work it avoids; optimize only when the simpler scan is meaningfully expensive.

### Message dispatch

Both server and client dispatch decoded wire payloads straight from ingress to one domain handler (`server/src/network/routing.rs`, `client/src/network/routing.rs`). Do not re-emit them as Bevy events: ingress is already a Bevy system with world access and each message has exactly one consumer, so an event layer adds no fan-out, scheduling, or parallelism — it has been tried, and it only obscured the receive-to-handler control flow. Reintroduce events only for a message that gains genuinely independent consumers. Bootstrap happens before the gameplay app runs: `main.rs` sends `CLogin`, waits for `SInit`, then builds and validates the app from it; the server spawns the body at login. The bootstrap rule that makes this safe without a readiness handshake is in the `protocol.rs` header. Once active, the server drops body-bound messages from a dead player, while `CPing`/`CAdmin`/`CChat` keep working through respawn.

### Protocol model

The message roles (bootstrap, state, cues, events) and the two lanes are defined in the top-of-file comment in `common/src/protocol.rs`; keep the detailed rules there only. Read it before adding a message. Most “X changed” state belongs in `SSnapshot`; a cue only for sub-tick latency, an edge-triggered side effect, or information a snapshot cannot carry; an event only when the snapshot cannot stand in for it.

### Gameplay systems

#### Death & respawn

`kill_player` in `server/src/combat/damage.rs` is the single death entry point; its function comment is authoritative for the sequence and callers. `explosions_system` drains player and actor blasts to a fixed point; death-blast kills award no kill credit (missile blasts do). `players_respawn_system` ticks the timer and spawns a fresh entity at a spawn zone.

#### Barriers & keys

Each `BarrierKindId` gets a dedicated Rapier collision group; `common/src/physics/world/colliders.rs` states the whole 32-group split once and the kind tables' `MAX` values follow from it. The id table comes from the selected map's ordered `barrier_kinds` in `gameplay.json`, is built once at server and client startup, and reaches the client inside `SInit.world.map.settings`. Players hold a sorted `Vec<BarrierKindId>` in `PlayerInfo.life.held_keys`; the character filter drops the matching groups so they pass through. Defined in `common/src/physics/world/colliders.rs` and `common/src/types/barrier_kind.rs`. The HUD draws one key slot per kind the map places a key for (`SInit.world.map.key_kinds`, from `MapConfig::key_kinds`), not per barrier kind.

#### Light bridges

Plate-powered walkways: translucent slabs that are a faint ghost by default and turn solid and lit while a bridge plate of their kind is held. Kinds come from the selected map's `bridge_kinds` in `gameplay.json` (required, nullable, like `barrier_kinds`) with colours in `assets.json`'s `bridge_kind_colors`. Authored per cell in the map editor and merged into maximal rectangles at compile time (`server/src/map/bridges.rs`), because the character controller reports a side contact at every collider seam. Each kind owns a Rapier collision group that the character, ground-probe, projectile-surface, and missile-lock filters include only while the kind is powered; everything deliberately static ignores them entirely, powered or not — line of sight, zapper beams, blasts, portal placement, ground probing for rain and scorch, and the missile `AirGraph`. Bridges set no `Cell` flags, so actors never walk them and item, spawn, and air-graph cells never see them. They ride `SSnapshot.plates` inside `PlateState`, which pairs the open barrier kinds with the powered bridge kinds and is the one value threaded into every collision filter on both sides.

#### Pressure plates

The exact occupancy, threshold, and edge-trigger rules live with `pressure_plates_system` in `server/src/map/pressure_plates.rs`, as does the solo rule (one logged-in player: holding plates are switches a fresh press flips); keep them authoritative there. There are three plate types — barrier, bridge, and firework — and the first two follow one shared threshold rule over `PlatePurpose`, while fireworks are momentary and never enter `PlateState`. Plates whose purpose solves a quest (`QuestKind::plate_purpose`) are inert and hidden on clients (`SSnapshot.locked_plate_purposes`) until that quest unlocks; the firework trigger records one `FireworksStarted` quest event, while `/firework` does not. The client renders every plate alike — `assets.json`'s `pressure_plate.panel` inset in `pressure_plate.frame` — so a plate's purpose is not visible.

#### Quests

The selected map's `quests` list is loaded into immutable `QuestCatalog`; `QuestBoard` holds only mutable session state. On login, the server assigns every currently unlocked quest and sends one batched `SQuestUpdates`; later assignments, progress, and completions use that same message with an explicit reason. Every update repeats the quest's complete client-visible definition, authored order, and current player/group state, so the client needs no catalog or earlier quest message; monotonic merging prevents stale progress or completion from regressing state. Group state also self-heals through `SSnapshot.quests`. Each quest has a `kind` (what advances it), a `scope`, points, and optionally `requires`. Scopes: `individual` (own progress on typed `PlayerInfo.session.quest_states`, own completion), `shared` (one pooled counter on the board; any player's event advances it; completes once for the group), `everyone` (own progress per player; completes for the group once every active player reached the threshold — the HUD shows `done/players`). `requires` hides a quest until the named `shared`/`everyone` quest completes; then it is assigned to every active player and to later joiners. Group completion is idempotent, normalizes shared progress to the threshold, credits every active player with that quest's points once, and reaches every active client as a completed quest update. `quests::record_event` is the entry point for cookies, actor kills, and the firework launch; `quests::recheck_everyone_quests` re-checks `everyone` quests on disconnect. Kinds advanced by a world event rather than a player (`fireworks`) must be `shared`; a kind may claim the plate purpose that solves it (`QuestKind::plate_purpose`), which keeps those plates locked until the quest unlocks. `/quest` lists the selected map's catalog and `/quest <id> [name|@a]` completes one by fiat (`quests::complete_quest`, after `unlock_quest` if it is still locked).

#### Character movement

Shared `step_character_movement` takes a `CharacterStep` that separates `control_velocity` from `external_displacement`. Ladder interaction reads only control velocity; knockback and client reconciliation ride external displacement so they can move a body without initiating or accelerating a climb. `step_player_movement` is the shared player policy layer: it merges held/open barrier passability, selects normal/low gravity, adds blast and portal momentum, runs the character step, and updates portal momentum. `player_control_velocity` is the shared resolver for speed-power-up and stun effects across authoritative movement, prediction, and reconciliation extrapolation. Input reaches the server as `CMove`, sent on change and every `SNAPSHOT_SECS` so a lost one heals; each one is broadcast as `SPlayerMove`. The one-shot actions (jump, shots) ride the reliable lane, since nothing could replay a lost one.

`common/src/physics/characters/support.rs` owns floor/perch probing, ground snap, and ramp projection; keep those support rules out of the movement orchestrator. Each step derives `CharacterSupport::{Airborne, Ground, Ladder}`; the server caches the last result in `PlayerInfo.life.fall_state` solely for fall tracking (`Ground` or `Ladder` ends the tracked fall). The motor never reads this support back, and it is not replicated.

#### Missiles

Ammo comes from `missile_pack` items (capped by `missiles.max_missiles`; a full player leaves the pack in the world, like an already-held key or a potion at full health — `pickup_has_effect` in `items/collection.rs`; reset on death). When missiles are enabled for the map, Q selects them and left click fires; the client crosshair locks any player/actor near the aim ray (`acquire_lock` in `common/src/physics/lock.rs`, with a configurable assist radius). There is no cooldown, so ammo is the rate limit; with `missiles.require_lock` off, an unlocked shot launches unguided along the aim (the shipped config requires a lock). All feedback (sound + the missile) waits for the server's `SMissileLaunch` so a rejected shot never orphans a cue.

The server owns the whole flight: launch at a random spread angle (with a clear-runway resample), direct homing with lead pursuit + cosmetic weave while sight is clear, `AirGraph` BFS waypoints when blocked, a swept proximity fuse, and detonation into `PendingExplosion::Missile` — the only blast that credits a killer. A missile that stops making progress self-detonates (`stall_secs`, via the shared `ProgressWatchdog`).

#### Portals

Q cycles every weapon enabled by the map (`client/src/input/weapons.rs` keeps the selected `WeaponMode` inside that loadout every frame, so the fire, lock-on, and crosshair systems trust it as-is). Login hands each player a portal slot (`PortalAssignments` in `server/src/portals/resources.rs`; `SInit.player.portal_access` seeds the client and `Player.portal_access` in every snapshot keeps it current): in `both` mode every slot owns a pair and left/right click send `CPortalShot` for ends A/B; in `single` mode adjacent slots share a pair, alternate A/B, and place their assigned end with left click. A lone `single` player holds both ends of their pair under the same pair id; a second login splits the ends again and removes the end the newcomer now controls (login runs `PortalMap::remove_access` on every fresh assignment, mirroring disconnect). `PortalMap` in the same file is the authoritative store, keyed by pair; the snapshot list and `PortalSet` are derived from it. The server raycasts the aim (`world_surface_along_ray`) and places the aperture on whatever geometry the ray hits — pure point + outward normal, no wall/floor/ceiling taxonomy (`server/src/portals/spawn.rs`). Placement is validated by the shared `compute_portal_placement`: the whole aperture needs solid backing and clear front space, and must not cover wall lights or — for standable portals — pressure plates; a failing shot bumps Portal-2-style to the nearest fitting spot (`nudged_center`) and only fizzles (client: dry-fire) when nothing fits. The client runs the same check before sending, so a miss dry-fires immediately; an accepted placement plays `portal_fire` flat for the shooter and spatially at the shooter for everyone else, while the server independently validates the player's assignment. Portals survive death; disconnect removes only the end(s) controlled by that player. They ride `SSnapshot.portals`, with `SPortalOpened` as the placement cue.

Traversal is true pass-through in `common/src/physics/portals/traversal.rs`; placement and overlap validation live beside it in `placement.rs`. Each linked aperture knows its backing colliders; while a character's body is in the aperture's front corridor, the movement step excludes them (`PortalSet::collision_exclusions`/`movement_collision_exclusions` → `CharacterEnvironment.portals`, players only — actors pass `None` and never fall through). Both sides call the shared `PortalSet::player_hop` and `CharacterPortalHop` state helpers: the tick the body's center crosses the plane, it continues from the paired end with position mapped continuously (aperture offset + penetration carried; the offset clamp is also what lets a steering player escape a fall chain), velocity split into vertical velocity, airborne `PortalMomentum`, and any separately mapped blast knockback, and the persisted movement intent mapped so held input immediately points out of the exit. Projectiles rank their own `projectile_hop` against other collision events in both flight sims. The aperture frame derives from the surface normal alone (world-up projected onto the plane; shooter yaw only where that degenerates), so ramps work unchanged. Two touches keep hand-placed floor/ceiling fall loops alive: the yaw of a vertical-normal placement snaps to quarter turns (`portal_placement_yaw`), and a body flying toward such an aperture is funneled toward its axis unless the player is steering (`PortalSet::funnel_displacement`, applied inside the shared movement step).

Client portal surfaces are off-axis render targets (`client/src/portals/render.rs`). Each active presenting camera roots its own view chains — the main camera through the shared surfaces, the rearview mirror through a replica set on its own layer — so the mirror sees through portals from its eye. Every complete root is built while the roots fit the budget; with more roots than budget, each presenter builds only the largest on screen, so the graph then follows the view. Views are built as deep as the budget could admit (budget − 1 hops, so one pair can fill it) under fixed camera/surface entity caps, deepest rendering first; `rendering.portal_view_budget` — the settings menu's "Portal views" — is the one user-facing knob: it caps the views each presenting camera renders per frame, largest on screen first, a nested view only under its admitted parent (0 = no see-through). Portal cameras render linear HDR with tonemapping and grading off, so the presenting camera tonemaps once. A view is active only while its aperture has a screen footprint at every hop of its chain — computed from this frame's camera, not read back from last frame's visibility — and a surface shows its view texture only while that view renders, its emissive glow otherwise. Each view renders only the on-screen part of its aperture (the aperture clipped by the view frustum) — the portal frustum is that sub-rectangle and the surface's `uv_transform` maps the disc onto it — so a texture never needs more pixels than its presenter has; its size is picked per axis from a power-of-two ladder off that footprint with shrink hysteresis, and each camera retains only its current and immediately previous immutable target buckets. The local player's meshes use a separate render layer: portal and shadow cameras include it, the first-person camera excludes it, and top-down includes it. Camera-facing player/actor labels use a main-view-only layer, and each scene camera gets its own layered sun/moon disc so mapped views preserve the celestial direction.

Crossings are never messaged: every client runs the same crossing for every simulated player, local and remote alike (`client/src/portals/prediction.rs`, right after predicted movement), exactly as it walks them up ramps — `SPortalOpened` keeps the shared geometry fresh, and the snapshot corrects a wrong guess about someone's motion near a plane. The local player's crossing additionally applies the Portal-style camera mapping (`client/src/portals/view.rs`: aim jumps to the upright mapped view, pitch carried; `PortalTransitBlend` decays the transient tilt). Reconciliation and remote overwrites stand down briefly after each simulated crossing (`RECON_TELEPORT_SUPPRESS_SECS`) because pre-crossing data drags a looping player to a stale phase. Crossings reset the fall tracker, so fall damage never carries through a portal.

#### Ladders

Freestanding climbable elements anchored on a grid edge (`{lower_level, col, row, side, levels}` in the map JSON, top-level like ramps) — no wall or floor required, deliberately dumb (nothing inspects surrounding geometry), and one-sided: only the FRONT, the rail side the normal points at, is a ladder. Nothing rides the wire beyond `MapLayout.ladders`, so prediction agrees for free.

The shared `step_character_movement` derives everything per tick from position + control intent against the front-only climb volume (`LadderVolume`, a plain AABB — no Rapier collider): pushing toward the rail plane ascends, pushing away descends (intent speed × `movement.ladder_climb_ratio`), idle latches, jump detaches, and an ascending or descending character gets an additive pull along the face toward the center axis that enough sideways movement can overpower. The plane is a fence for front-side characters up to the top landing, open above it (`clamp_move_at_ladder_plane`). From the back a walker passes straight through and emerges on the front face — that is the mid-ladder mount from a balcony behind it — and the volume's overshoots at both ends make the top crest and the bottom grab work.

#### Weather & lighting

Both are continuous state in every snapshot, seeded from the map's `weather`/`lighting` mode (a concrete state, or `auto` for the global cycle). `weather_system` runs the rain cycle; intensity rides `SSnapshot.rain_intensity` and the client smooths + renders it (`vfx/rain.rs`).

Lighting is separate — rain does not dim the world — and the wire speaks preset names: `SSnapshot.lighting` is a `LightingBlend {from, to, blend}` between two named client-side looks (`bright`/`dim`/`dark`, hand-tuned in `client.json`; a plain preset is the degenerate `from == to`). `light_cycle_system` runs the cycle — a wrapping clock over `cycles.lighting` (hold at each present stop of `bright_secs`/`dim_secs`/`dark_secs` — any two or three — fading between adjacent stops; `blend_at` is the pure timeline→blend map). The client's `lighting_blend_system` resolves the names and eases every channel toward the blended look in look space — intensity channels in log space, so fades are perceptually even — and cycle steps, segment crossings, and admin jumps all fade with one mechanism.

`/weather` and `/light` report current state; `/weather rain|clear|auto` and `/light bright|dim|dark|auto|<0..1>|<from> <to> <0..1>` hold a state (named looks are absolute, numeric holds are cycle-relative) or resume the cycle continuously.

#### Actor lifecycle

`actors_removal_system` handles both health-zero ("killed", with explosion blast + `SActorDeath`) and fall ("silent"). `actors_respawn_system` batch-refills every missing slot in a zone after its kind's `respawn_secs`; `null` disables refills. Replacements are queued into `PendingActorSpawns` with ids, unoccupied spots, and headings reserved. `actors_pending_spawn_system` materializes each entry after `actors.settings.spawn_warning_secs`; `SInit` carries that static duration once, while each snapshot spawn carries only its remaining time. During the window the actor doesn't exist server-side and clients render a ghost from `spawning_actors`.

#### Actor AI

Contact, beam, and contact+beam attackers have separate decision controllers selected by the tagged `attack` config. Decisions run at 10 Hz; route following, collision, beam damage, and timers run at the 30 Hz tick. Spawn zones expand through the floor-walking `NavGraph` by `roam_steps` into a precomputed home/roam territory; active combat may use the entire reachable nav component. Engagement routes string-pull BFS cell waypoints across footprint-safe flat floor.

Anti-jam handling: two route-construction rules (`NavGraph::anchor_route_start`; `waypoint_passed` in `behavior/tick.rs`) plus stall recovery via the shared `ProgressWatchdog` — a stalled actor hops to a random neighboring cell before rethinking (`shake_loose`). The WHYs live as comments at those definitions.

Threats are acquired by LOS within spherical vision and retained for `actors.settings.threat_memory_secs` after contact is lost, then actors return home. Reachable attacks outrank evasion; contact actors evade unreachable players, zappers evade during beam cooldown, and the reaper (contact+beam) moves by contact rules while firing its beam opportunistically (the beam target lives in `BeamState::Firing`). Evasion picks stable cover in a bounded local search and revalidates it as threats move.

### Conventions

- This is pre-release software: JSON configs, map files, and the wire protocol are intentionally unversioned, and breaking changes are allowed. Update every producer, consumer, fixture, and checked-in config together; do not add version fields, compatibility branches, or migrations unless this policy changes. The one exception is the gitignored `config/client/client_local.json`, which git cannot update — see its entry above.
- Entity IDs are newtype wrappers: `PlayerId(u32)`, `ActorId(u32)`, `ItemId(u32)`, `MissileId(u32)`, `BarrierKindId(u16)`.
- Bevy resources `PlayerMap` / `ActorMap` / `ItemMap` / `MissileMap` map IDs to entities on both sides.
- The player, actor, and missile client reconciliation pipelines are three deliberate copies — do not unify them.
- Tokio mpsc channels bridge async QUIC I/O with Bevy's sync systems.
- Coordinates: Bevy Y-up `(x, y, z)`, units in meters.
- Wire format: `bincode` 2 (binary).
- `score` (server `PlayerInfo.session.score`, client `PlayerInfo.score`, wire `Player.score`) accumulates per-event deltas from the server config's `scoring` block and persists across deaths. Don't confuse with `health`.
- "Dead": on the server, `PlayerInfo::is_dead()` means its per-life lifecycle is `Dead`, which owns the respawn timer and no entity. On the client, `LocalPlayerInfo.is_dead` is a separate flag. Don't try to unify the two — they live in different crates.
- Keep gameplay concepts (`Wall`, `Floor`, `Ramp`, `Barrier`, items, spawn zones) in map/protocol types; keep reusable movement/collision behaviour in `common::physics`.
- Mesh UVs are computed from world position, not local position. Floor/wall/ramp builders in `client/src/map/spawn/` take `world_center` (and `rotation` for walls); each vertex's UV is `(world_center + rotation * local_pos) · uv_axis / tile_size`. New mesh builders should follow the same pattern.

## Map editor (`tools/editor.py`)

The canvas IS the UI. Do not add coordinate readouts, row/col numbers, or
status-bar grid info — if something needs explaining, it should be drawn on
the canvas itself. PySide6 with mouse-driven click/drag interactions per
mode (floors, grass, walls, ramps, ladders, barriers, light bridges, spawn
zones, items, materials, lights, pressure plates). It reads the edited map's
barrier and bridge kinds
from `config/server/gameplay.json`; colours and material aliases come from
`config/client/assets.json`.

## Adding a texture

Texture sets are freepbr.com UE packs. To add one:

1. Copy the pack's directory as-is into `client/assets/textures/<name>-ue/`.
2. Build the packed metallic-roughness map Bevy wants (needs ImageMagick): `client/assets/textures/combine_metallic_roughness.sh <dir>/<name>_roughness.png <dir>/<name>_metallic.png` writes `<name>_metallic-roughness.png` next to them. `multiply_intensity.sh <metallic-roughness.png> [roughness_add] [metallic_multiply]` retunes it afterwards (keeps a `.original.png`).
3. Add a `materials.<name>` entry in `config/client/assets.json`: `textures.base_color` (`_albedo`), `normal` (`_normal-dx`), `occlusion` (`_ao`), `metallic_roughness`; `tile_size` in meters; `repeat` and `linear_data_textures` true.
4. Reference it — map faces may only name `aliases` entries (add one), items use `item_materials`, fixtures use `ladder` / `pressure_plate.panel` / `pressure_plate.frame` (each names a `material`).
5. `cargo test --release -p client referenced_assets_exist_case_exactly` catches path and case typos.

## Coding style

- Rust edition 2024. Format with `cargo fmt` (see `rustfmt.toml`).
- Workspace lints (root `Cargo.toml`): `unsafe_code = "forbid"`; `unwrap_used = "warn"` — prefer `expect("…")` with a message, or proper error handling; `todo = "warn"`.
- Naming: `snake_case` functions/modules, `CamelCase` types, `SCREAMING_SNAKE_CASE` constants.
- Do not introduce `Arc` just to make borrowing or ownership convenient. Reserve it for genuine cross-thread shared ownership or APIs that require it (such as Quinn); otherwise prefer references, owned values, IDs/indices, or restructuring ownership.
- Use `assert!` / `assert_eq!` / `assert_ne!` for invariants — never `debug_assert!`. Only release builds run, so `debug_assert!` is a no-op.
- `mod.rs` files contain only `mod` declarations and `pub use` re-exports (attributes like `#[cfg(test)]` on them are fine) — no functions, types, or impls. Code that would land in a `mod.rs` goes in a named sibling file (e.g. `plugin.rs`) and gets re-exported.
- A module with submodules uses `<module_name>/mod.rs`; never pair `<module_name>.rs` with a sibling `<module_name>/` directory.
- Default to writing no comments. Only add one when the WHY is non-obvious: a hidden constraint, a subtle invariant, a workaround. Don't explain WHAT well-named code already says.
- When a code comment is the authoritative description of a rule or invariant, point to it from `AGENTS.md` instead of duplicating it; keep one source of truth.

## Testing

`cargo test --release --workspace` is the canonical command. Unit tests live
next to the module they cover under `#[cfg(test)] mod tests`. There are no
`tests/` integration-test directories in this repo. Name tests after what
they assert (e.g. `lethal_hit_returns_true`, `barrier_collision_group_is_unique_per_kind`).
The map editor's headless `unittest` suite lives in `tools/tests/` and covers
its pure geometry, normalization, resizing, and validation helpers.

## Documentation

- `README.md` is for players: what the game has, how to run it, the controls. One plain line per gameplay feature — no rules, mechanics, config paths, or version numbers. It must not turn into a technical spec sheet.
- `AGENTS.md` is for developers and agents: systems, rules, and pointers into code, written at the same depth as the sibling entries.

## Commits & pull requests

- Short, imperative summaries.
- PRs: describe behaviour impact on client/server, include repro steps or screenshots for client-facing changes, call out protocol or asset changes explicitly (they're breaking by default).

## When in doubt, read

- **Protocol & message taxonomy** — top doc comment of `common/src/protocol.rs`.
- **Collision groups & character filters** — `common/src/physics/world/colliders.rs`.
- **Death/respawn pipeline** — `server/src/combat/damage.rs` (`kill_player`), `server/src/players/respawn.rs` (`players_respawn_system`), `client/src/network/players/sync.rs` (snapshot diff).
- **Map data shape** — `common/src/types/` and `config/server/maps/hotel.json`.
- **Per-kind gameplay tuning** — body geometry, per-map movement, server behaviour, health, and damage in `config/server/gameplay.json`.
- **Missile guidance & air routing** — `server/src/missiles/guidance.rs` and `server/src/missiles/air_graph.rs`.
- **Admin commands** — `server/src/network/admin/command.rs` (`HELP_TEXT` is the catalog) and `server/src/network/admin/execute.rs`.

## Security & assets

- **Current threat model:** development and private/LAN play assume cooperative clients. Abuse hardening — client rate limits, per-tick ingress budgets, bounded/backpressured network queues, flood protection, and admin authorization — is intentionally deferred. Revisit it before any public release or publicly accessible server.
- `client/assets/` are not open source — replace before publishing a fork.
- `cert.pem` / `key.pem` are local-dev only. Do not commit production keys.
