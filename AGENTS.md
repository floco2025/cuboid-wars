# Repository Guidelines

## Project structure

Rust workspace with three crates:

- **`common/`** — shared between client and server.
  - `protocol.rs` — all `ClientMessage` / `ServerMessage` variants and wire structs. Read the top-of-file doc comment before adding a new message — it lays out the bootstrap / snapshot / real-time-intent / one-shot-cue / per-client-state / diagnostic taxonomy that decides where new messages go.
  - `network.rs` — `MessageStream` abstraction over QUIC.
  - `physics/` — shared player/projectile movement, collision world (incl. per-kind barrier collision groups, plus the non-solid ladder climb volumes in `world/ladders.rs`), barrier passability, spawn validation helpers, and missile lock-on acquisition (`lock.rs`, used by the client crosshair and server fire validation).
  - `types/` — shared markers, IDs, positions, movement states, map layout types (`types/map_layout.rs`), items/power-ups, snapshots, `BarrierKindTable`.
  - `map/` — shared map behaviour: level classification + ramp surfaces (`levels.rs`), grid↔world conversion (`geometry.rs`, `MapGeometry`).
  - `health.rs`, `constants.rs` — the `Health` type with its operations, and gameplay constants.
- **`server/`** — authoritative headless server (Bevy `MinimalPlugins`).
  - `app.rs` builds the ECS app and installs function-style domain plugins, matching the client pattern; each domain's `plugin.rs` owns its system registration. `schedule.rs` defines their cross-domain tick order (`Prepare` → ingress → behaviour/movement → combat damage/removal/explosions → lifecycle/maintenance → snapshot). Deferred commands are flushed after preparation, ingress, and combat, and again immediately before snapshots, so ID maps never expose unmaterialized entities to network collection.
  - `actors/`, `characters/`, `items/`, `players/`, `projectiles/` — server-side domain systems. Each domain keeps its Bevy resources in its own `resources.rs` (`players/resources.rs` holds `PlayerInfo`/`PlayerMap` and inventory/power-up state; `players/quests.rs` owns quest state and mutations, while `players/falling.rs` owns `PlayerFallState`). Actor AI uses separate contact/beam controllers over `Roam`/`Engage`/`Evade`/`ReturnHome` modes; navigation precomputes graph-based home/roam territories from spawn zones, and every mode uses the same waypoint route follower.
  - `network/` — the whole networking concern: async QUIC transport (`transport.rs`, accepts connections), Bevy-side message dispatch (`incoming.rs`/`messages.rs`), login, snapshot broadcast (`snapshot.rs`/`broadcast.rs`), and the admin command executor (`admin.rs` — parses `CAdmin` strings like `/give missiles` or `/firework`; `/help` lists all).
  - `missiles/` — the seeking-missile weapon: fire validation + launch (`spawn.rs`), guidance (`guidance.rs` — lead pursuit, serpentine weave, proximity fuse, obstacle avoidance), movement/detonation (`movement.rs`), and `air_graph.rs` — a full-3D BFS over the map's airspace (per-cell-per-level air volumes + a sky layer), deliberately separate from the actors' floor-walking `NavGraph`.
  - `combat/` — damage application + `kill_player`/`kill_actor` (`damage.rs`, the one-stop death sequence) and blast resolution (`explosions.rs`, with `PendingExplosions` in `resources.rs`; missile blasts carry shooter kill credit).
  - `map/` — converts map definitions into runtime layout: cells/edges, floors, walls, ramps, barriers, lights, masks, segments; the runtime map model lives in `map/resources.rs`. Also the weather and day/night schedulers (`weather.rs` and `light_cycle.rs` — driven by the global `weather_cycle`/`lighting_cycle` configs and each map's `weather`/`lighting` mode, broadcast as `SSnapshot.rain_intensity`/`light_level`, overridable via `/weather` and `/light`).
  - `watchdog.rs` — `ProgressWatchdog`, the one stall detector shared by actors (shake loose to a neighboring cell) and missiles (self-detonate).
  - `config/` — server config split by concern: QUIC setup (`network.rs`), gameplay registry + tuning (`gameplay.rs`), per-actor-kind cluster (`actors.rs`).
  - Runs at 30 Hz via a manual `app.update()` loop.
- **`client/`** — Bevy renderer, input, UI.
  - `app.rs` builds the Bevy app, loads and validates client config, installs shared resources, and registers domain plugins. `main.rs` owns CLI parsing, QUIC/Tokio setup, login, and process lifecycle.
  - `network/` — consumes `ServerMessage`; each domain is a `handlers.rs` + `sync.rs` pair (`players/`, `actors/`, `items/`, `missiles/`), with quest handlers in `quests.rs` and the snapshot diff dispatched from `snapshot.rs`.
  - `players/`, `actors/`, `characters/`, `items/`, `projectiles/`, `missiles/` — client-side domain systems (`transform_sync.rs` files hold the per-frame interpolation systems; the shared character animation observer lives in `characters/animation.rs`). `missiles/` holds the procedural missile mesh, dead-reckoning movement, and the crosshair lock-on detector (`lock_on.rs`).
  - `input/`, `cameras/`, `ui/`, `vfx/` — client-only interaction, rendering support, presentation. The explosion effect is one subsystem in `vfx/explosion/` (assets, spawn, animation, scorch, shards, smoke); `vfx/` also holds the zapper laser beam (`laser.rs`), rain (`rain.rs`), missile exhaust (`exhaust.rs`), and the shared GPU particle clouds (`particles.rs` — every particle in the game is a cube; keep new effects spark-sized or they read as floating boxes), and the seeded client-side firework show (`firework.rs`, played on the `/firework` cue). The chat + admin console (Enter or `/`, ↑/↓ history) lives in `ui/console.rs` — slashless lines are chat (`CChat` → broadcast `SChat`), `/` lines are admin commands.
  - `map/` — client map rendering and geometry spawning; procedural grass (incl. burn response) in `map/grass/`, skybox in `map/skybox.rs`.
  - `config/` — JSON-backed settings split by concern (`settings.rs` root + `audio`/`camera`/`hud`/`rendering`/`vfx`) plus the asset set (`assets.rs`).

Other notable paths:

- `tools/editor.py` — launcher for the PySide6 map editor (code lives in `tools/map_editor/`); takes a map name and edits `config/server/maps/<name>.json`.
- `client/assets/` — 3D models, textures, audio.
- `config/client/assets.json` — hand-edited asset set (materials, material rules, models, sounds, barrier kind colours).
- `config/client/render.json` — client-only render/debug settings.
- `config/common/gameplay.json` — shared simulation tuning loaded by client and server (player/actor physics, actor `roam_speed`/`active_speed`, player jump speed, the `projectiles` flight block, the `ladders` climb ratio, and the `missiles` block both sides need: lock range, aim-assist radius, ammo cap, blast radius).
- `config/server/gameplay.json` — server-only gameplay tuning, including global `actor_settings` and per-kind actor vision, graph-step roam territories, explicit tagged contact/beam/contact_beam attacks, and nullable respawn delays, plus the consolidated `scoring` block (every point value in the game: player kill/death, cookie, per-actor-kind `actor_hit`/`actor_kill`, per-quest `quest_completed` — the maps must cover exactly the defined actor kinds and quest ids), the global `weather_cycle`/`lighting_cycle` blocks (rain cadence; bright/dark cadence), and the named-map registry: `maps` maps each name to its per-map settings (`skybox`, `gravity`, `low_gravity`, optional `random_items` spawn pool, and the `weather` (`clear`|`rain`|`auto`) and `lighting` (`bright`|`dim`|`dark`|`auto`) modes — a concrete value holds that state, `auto` runs the global cycle), `default_map` picks the one to load (`--map <name>` overrides). `placed_items.respawn_secs` sets the per-type reappear delay for map-placed items; the `missiles` block holds server-only flight/guidance/damage tuning.
- `config/server/maps/` — one map JSON per named map (geometry, zones, and placed `items`; per-map tuning lives in the `maps` registry).
- `cert.pem` / `key.pem` — local-dev TLS for QUIC (not production-safe).
- `launch_clients.sh` — spawns N tiled windowed clients for local multiplayer testing (`./launch_clients.sh [num_clients] [lag_ms]`, macOS).
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

### Protocol model

Server→client messages have four roles, documented at the top of `common/src/protocol.rs`:

1. **Bootstrap** (`SInit`) — once per connection.
2. **Snapshot** (`SSnapshot`) — periodic full durable state, broadcast at `SNAPSHOT_HZ` (4 Hz). **Sole vehicle for player/actor/item presence**: a player appears in the first snapshot they show up in and disappears in the first they're absent from. No `SLogin`/`SLogoff` — login, logout, death, and respawn all surface here. Self-healing if a packet drops. Presence includes pre-presence: `spawning_actors` carries reserved actor spawns during their beam-in warning window. Projectiles are the deliberate exception: because they are fast, short-lived, and numerous, they are replicated as shot intents (`SPlayerShot`) rather than snapshot entities. Clients simulate them for presentation only; authoritative hit/death logic comes from the server. Missiles are NOT that exception: they fly for seconds and steer server-side, so they are full snapshot entities (`SSnapshot.missiles`) reconciled like actors — clients dead-reckon the broadcast velocity, with `SMissileLaunch`/`SMissileMove`/`SMissileDeath` as the latency cues.
3. **One-shot cues** — short messages for things the snapshot can't carry (sub-tick latency or edge-triggered side-effects). Examples: `SPlayerShot` (projectile presentation), `SPlayerHit` (direction-bearing camera shake), `SPlayerDeath`/`SActorDeath` (immediate VFX + entity teardown), `SPlayerStatus` (power-up sound at the transition).
4. **Per-client state events** — durable per-player state other clients don't need (e.g. quest assignment/progress), unicast to the affected player with no snapshot fallback. Quest progress is absolute and max-merged; the client buffers progress/completion received before assignment until the assignment supplies display metadata.

When adding a new server→client message: pick the smallest role that fits. Most "X changed" belongs in `SSnapshot`. Only add a one-shot if (a) sub-tick latency matters, (b) the cue is edge-triggered with a one-time side effect, or (c) it carries information the snapshot can't.

### Gameplay systems

#### Death & respawn

`kill_player` in `server/src/combat/damage.rs` is the single entry point — clears per-life state on `PlayerInfo`, arms `death_timer`, queues the death explosion into `PendingExplosions`, despawns the entity, broadcasts `SPlayerDeath`. Called from projectile lethal hits, explosion blasts, and falls below `CHARACTER_FALL_DEATH_Y` (`players_fall_death_system`). `explosions_system` drains player and actor blasts to a fixed point; blast kills award no kill credit. `players_respawn_system` ticks the timer and spawns a fresh entity at a spawn zone.

#### Barriers & keys

Each `BarrierKindId` gets a dedicated Rapier collision group (bits 3..31, max 29 kinds). Players hold a sorted `Vec<BarrierKindId>` in `PlayerInfo.held_keys`; the character filter drops the matching groups so they pass through. Defined in `common/src/physics/world/colliders.rs` and `common/src/types/barrier_kind.rs`.

#### Character movement

Shared `step_character_movement` takes a `CharacterStep` that separates `control_velocity` from `external_displacement`. Ladder interaction reads only control velocity; knockback and client reconciliation ride external displacement so they can move a body without initiating or accelerating a climb. `player_control_velocity` is the shared resolver for speed-power-up and stun effects across authoritative movement, prediction, and reconciliation extrapolation.

`common/src/physics/characters/support.rs` owns floor/perch probing, ground snap, and ramp projection; keep those support rules out of the movement orchestrator. Each step derives `CharacterSupport::{Airborne, Ground, Ladder}`; the server caches the last result in `PlayerInfo.movement_support` solely for fall tracking (`Ground` or `Ladder` ends the tracked fall). The motor never reads this support back, and it is not replicated.

#### Missiles

Ammo comes from `missile_pack` items (capped by `missiles.max_missiles`; a full player leaves the pack in the world, like an already-held key; reset on death). The client crosshair locks any player/actor near the aim ray (`acquire_lock` in `common/src/physics/lock.rs`, with a configurable assist radius) and F fires — no cooldown, ammo is the rate limit; with `missiles.require_lock` off (the default), an unlocked shot launches unguided along the aim. All feedback (sound + the missile) waits for the server's `SMissileLaunch` so a rejected shot never orphans a cue.

The server owns the whole flight: launch at a random spread angle (with a clear-runway resample), direct homing with lead pursuit + cosmetic weave while sight is clear, `AirGraph` BFS waypoints when blocked, a swept proximity fuse, and detonation into `PendingExplosion::Missile` — the only blast that credits a killer. A missile that stops making progress self-detonates (`stall_secs`, via the shared `ProgressWatchdog`).

#### Ladders

Freestanding climbable elements anchored on a grid edge (`{lower_level, col, row, side, levels}` in the map JSON, top-level like ramps) — no wall or floor required, deliberately dumb (nothing inspects surrounding geometry), and one-sided: only the FRONT, the rail side the normal points at, is a ladder. Nothing rides the wire beyond `MapLayout.ladders`, so prediction agrees for free.

The shared `step_character_movement` derives everything per tick from position + control intent against the front-only climb volume (`LadderVolume`, a plain AABB — no Rapier collider): pushing toward the rail plane ascends, pushing away descends (intent speed × `ladders.climb_speed_ratio`), idle latches, jump detaches, and the plane is a fence for front-side characters up to the top landing, open above it (`clamp_move_at_ladder_plane`). From the back a walker passes straight through and emerges on the front face — that is the mid-ladder mount from a balcony behind it — and the volume's overshoots at both ends make the top crest and the bottom grab work.

#### Weather & lighting

Both are continuous state in every snapshot, seeded from the map's `weather`/`lighting` mode (a concrete state, or `auto` for the global cycle). `weather_system` runs the rain cycle; intensity rides `SSnapshot.rain_intensity` and the client smooths + renders it (`vfx/rain.rs`).

Lighting is separate — rain does not dim the world — and the wire speaks preset names: `SSnapshot.lighting` is a `LightingBlend {from, to, blend}` between two named client-side looks (`bright`/`dim`/`dark`, hand-tuned in `client.json`; a plain preset is the degenerate `from == to`). `light_cycle_system` runs the cycle — a wrapping clock over `lighting_cycle` (hold at each present stop of `bright_secs`/`dim_secs`/`dark_secs` — any two or three — fading between adjacent stops; `blend_at` is the pure timeline→blend map). The client's `lighting_blend_system` resolves the names and eases every channel toward the blended look in look space — intensity channels in log space, so fades are perceptually even — and cycle steps, segment crossings, and admin jumps all fade with one mechanism. The login-time snapshot carries the live values, so a dark map never flashes bright on connect.

`/weather` and `/light` report current state; `/weather rain|clear|auto` and `/light bright|dim|dark|auto|<0..1>|<from> <to> <0..1>` hold a state (named looks are absolute, numeric holds are cycle-relative) or resume the cycle continuously.

#### Actor lifecycle

`actors_removal_system` handles both health-zero ("killed", with explosion blast + `SActorDeath`) and fall ("silent"). `actors_respawn_system` batch-refills every missing slot in a zone after its kind's `respawn_delay_secs`; `null` disables refills. Replacements are queued into `PendingActorSpawns` with ids, unoccupied spots, and headings reserved. `actors_pending_spawn_system` materializes each entry after the global `actor_settings.spawn_warning_secs` beam-in window; during the window the actor doesn't exist server-side and clients render a ghost from the snapshot's `spawning_actors`.

#### Actor AI

Contact, beam, and contact+beam attackers have separate decision controllers selected by the tagged `combat.attack` config. Decisions run at 10 Hz; route following, collision, beam damage, and timers run at the 30 Hz tick. Spawn zones expand through the floor-walking `NavGraph` by `roam_steps` into a precomputed home/roam territory; active combat may use the entire reachable nav component. Engagement routes string-pull BFS cell waypoints across footprint-safe flat floor.

Anti-jam handling: two route-construction rules (`NavGraph::anchor_route_start`; `waypoint_passed` in `behavior/tick.rs`) plus stall recovery via the shared `ProgressWatchdog` — a stalled actor hops to a random neighboring cell before rethinking (`shake_loose`). The WHYs live as comments at those definitions.

Threats are acquired by LOS within spherical vision and retained for `actor_settings.threat_memory_secs` after contact is lost, then actors return home. Reachable attacks outrank evasion; contact actors evade unreachable players, zappers evade during beam cooldown, and the reaper (contact+beam) moves by contact rules while firing its beam opportunistically (the beam target lives in `BeamState::Firing`). Evasion picks stable cover in a bounded local search and revalidates it as threats move.

### Conventions

- This is pre-release software: JSON configs, map files, and the wire protocol are intentionally unversioned, and breaking changes are allowed. Update every producer, consumer, fixture, and checked-in config together; do not add version fields, compatibility branches, or migrations unless this policy changes.
- Entity IDs are newtype wrappers: `PlayerId(u32)`, `ActorId(u32)`, `ItemId(u32)`, `MissileId(u32)`, `BarrierKindId(u16)`.
- Bevy resources `PlayerMap` / `ActorMap` / `ItemMap` / `MissileMap` map IDs to entities on both sides.
- The player, actor, and missile client reconciliation pipelines are three deliberate copies — do not unify them.
- Tokio mpsc channels bridge async QUIC I/O with Bevy's sync systems.
- Coordinates: Bevy Y-up `(x, y, z)`, units in meters.
- Wire format: `bincode` 2 (binary).
- `score` (server `PlayerInfo.score`, client `PlayerInfo.score`, wire `Player.score`) accumulates per-event deltas from the server config's `scoring` block and persists across deaths. Don't confuse with `health`.
- "Dead": on the server, `PlayerInfo::is_dead()` ≡ `death_timer.is_some()`. On the client, `LocalPlayerInfo.is_dead` is a separate flag (client doesn't see the timer). Don't try to unify the two — they live in different crates.
- Keep gameplay concepts (`Wall`, `Floor`, `Ramp`, `Barrier`, items, spawn zones) in map/protocol types; keep reusable movement/collision behaviour in `common::physics`.
- Mesh UVs are computed from world position, not local position. Floor/wall/ramp builders in `client/src/map/spawn/` take `world_center` (and `rotation` for walls); each vertex's UV is `(world_center + rotation * local_pos) · uv_axis / tile_size`. New mesh builders should follow the same pattern.

## Map editor (`tools/editor.py`)

The canvas IS the UI. Do not add coordinate readouts, row/col numbers, or
status-bar grid info — if something needs explaining, it should be drawn on
the canvas itself. PySide6 with mouse-driven click/drag interactions per
mode (floors, grass, walls, ramps, ladders, barriers, spawn zones, items,
materials, lights, pressure plates).

## Coding style

- Rust edition 2024. Format with `cargo fmt` (see `rustfmt.toml`).
- Workspace lints (root `Cargo.toml`): `unsafe_code = "forbid"`; `unwrap_used = "warn"` — prefer `expect("…")` with a message, or proper error handling; `todo = "warn"`.
- Naming: `snake_case` functions/modules, `CamelCase` types, `SCREAMING_SNAKE_CASE` constants.
- Use `assert!` / `assert_eq!` / `assert_ne!` for invariants — never `debug_assert!`. Only release builds run, so `debug_assert!` is a no-op.
- `mod.rs` files contain only `mod` declarations and `pub use` re-exports (attributes like `#[cfg(test)]` on them are fine) — no functions, types, or impls. Code that would land in a `mod.rs` goes in a named sibling file (e.g. `plugin.rs`) and gets re-exported.
- Default to writing no comments. Only add one when the WHY is non-obvious: a hidden constraint, a subtle invariant, a workaround. Don't explain WHAT well-named code already says.

## Testing

`cargo test --release --workspace` is the canonical command. Unit tests live
next to the module they cover under `#[cfg(test)] mod tests`. There are no
`tests/` integration-test directories in this repo. Name tests after what
they assert (e.g. `lethal_hit_returns_true`, `barrier_collision_group_is_unique_per_kind`).
The map editor's headless `unittest` suite lives in `tools/tests/` and covers
its pure geometry, normalization, resizing, and validation helpers.

## Commits & pull requests

- Short, imperative summaries.
- PRs: describe behaviour impact on client/server, include repro steps or screenshots for client-facing changes, call out protocol or asset changes explicitly (they're breaking by default).

## When in doubt, read

- **Protocol & message taxonomy** — top doc comment of `common/src/protocol.rs`.
- **Collision groups & character filters** — `common/src/physics/world/colliders.rs`.
- **Death/respawn pipeline** — `server/src/combat/damage.rs` (`kill_player`), `server/src/players/respawn.rs` (`players_respawn_system`), `client/src/network/players/sync.rs` (snapshot diff).
- **Map data shape** — `common/src/types/` and `config/server/maps/hotel.json`.
- **Per-kind gameplay tuning** — `config/common/gameplay.json` (shared) and `config/server/gameplay.json` (server-only).
- **Missile guidance & air routing** — `server/src/missiles/guidance.rs` and `server/src/missiles/air_graph.rs`.
- **Admin commands** — `server/src/network/admin.rs` (`HELP_TEXT` is the catalog).

## Security & assets

- **Current threat model:** development and private/LAN play assume cooperative clients. Abuse hardening — client rate limits, per-tick ingress budgets, bounded/backpressured network queues, flood protection, and admin authorization — is intentionally deferred. Revisit it before any public release or publicly accessible server.
- `client/assets/` are not open source — replace before publishing a fork.
- `cert.pem` / `key.pem` are local-dev only. Do not commit production keys.
