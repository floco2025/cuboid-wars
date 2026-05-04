# Repository Guidelines

This file is loaded by Claude Code, Codex, Cursor, and similar coding agents.

## Project Structure

Rust workspace with three crates:

- **`common/`** — shared between client and server.
  - `protocol.rs` — all `ClientMessage` / `ServerMessage` variants and wire structs.
  - `net.rs` — `MessageStream` abstraction over QUIC.
  - `physics/` — shared player/projectile movement, collision world, and spawn validation helpers.
  - `types/` — shared markers, IDs, positions, movement states, map layout types, and snapshots.
  - `map.rs`, `constants.rs` — shared map helpers and gameplay constants.
- **`server/`** — authoritative headless server (Bevy `MinimalPlugins`).
  - `actors/`, `characters/`, `items/`, `players/`, `projectiles/` — server-side domain systems.
  - `network/` — accepts connections, handles client messages, and broadcasts snapshots.
  - `resources/` — Bevy resources split by domain.
  - `map/` — converts map definitions into runtime layout: cells/edges, floors, walls, ramps, lights, masks, and segments.
  - Runs at 30 Hz via a manual `app.update()` loop.
- **`client/`** — Bevy renderer, input, UI.
  - `network/` — consumes `ServerMessage`, spawns/updates entities, and runs transport I/O.
  - `players/`, `actors/`, `characters/`, `items/`, `projectiles/` — client-side domain systems.
  - `input/`, `cameras/`, `ui/`, `vfx/`, `animations/` — client-only interaction, rendering support, and presentation.
  - `map/` — client map rendering and geometry spawning.

Other notable paths:
- `tools/editor.py` — PySide6 map editor for `config/server/map.json`.
- `tools/preview.py` — ASCII map preview/validation helper.
- `client/assets/` — 3D models, textures, audio.
- `config/client/assets.json` — hand-edited asset set for materials, material rules, models, and sounds.
- `config/client/render.json` — client-only render/debug settings.
- `config/common/gameplay.json` — shared simulation tuning loaded by client and server.
- `config/server/gameplay.json` — server-only gameplay tuning.
- `config/server/map.json` — default map source JSON.
- `cert.pem` / `key.pem` — local-dev TLS for QUIC.
- `launch_clients.sh` — spawns multiple windowed clients for local multiplayer testing.
- `bacon.toml` — `bacon` job definitions (`check`, `clippy`, `build`, `test`).

## Build, Run, Lint, Format

```bash
cargo build                                       # workspace, debug
cargo build --release
cargo run --bin server                            # default bind 127.0.0.1:8080
cargo run --bin server -- --bind 0.0.0.0:8080
cargo run --bin client                            # default connects to 127.0.0.1:8080
cargo run --bin client -- --server 192.168.1.100:8080 --name "PlayerName"
python3 tools/editor.py                           # edit config/server/map.json
python3 tools/preview.py                          # print ASCII map preview
cargo clippy                                      # pedantic + nursery + cargo lint groups
cargo fmt
```

## Architecture Notes

**Server is authoritative for**: player and actor positions (with reconciliation state in movement messages and snapshots), all collisions, item spawning/collection, map generation (sent once via `SInit` on connect).

**Client owns**: input, local movement prediction, rendering, camera, UI.

**Networking/prediction invariant**: `SUpdate` is a periodic full snapshot and recovery path, not the primary mechanism for responsive movement changes. Movement prediction must continue to behave reasonably even if `SUpdate` is sent only every several seconds. Any server-side movement change that affects prediction before the next snapshot must be sent as an immediate event message. Player move intent changes use `CPlayerMoveIntent` / `SPlayerMoveIntent`; server-authored actor move intent changes must likewise use `SActorMoveIntent` rather than waiting for `SUpdate`. This rule is movement-specific; non-predicted state such as item spawns may remain snapshot-only.

**Conventions**:
- Entity IDs are newtype wrappers: `PlayerId(u32)`, `ItemId(u32)`.
- Bevy resources `PlayerMap` / `ActorMap` / `ItemMap` map IDs to entities (server- and client-side).
- Tokio mpsc channels bridge async QUIC I/O with Bevy's sync systems.
- Coordinates: Bevy Y-up `(x, y, z)`, units in meters.
- Wire format: `bincode` 2 (binary).
- The default map source is `config/server/map.json`; the server turns it into `MapLayout`, sends that to clients, and both sides build shared collision/rendering state from it.
- Keep gameplay concepts (`Wall`, `Floor`, `Ramp`, items, player spawn fields) in map/protocol types; keep reusable movement/collision behavior in `common::physics`.

## Coding Style

- Rust edition 2024. Format with `cargo fmt` (see `rustfmt.toml`).
- Workspace lints (root `Cargo.toml`): `unsafe_code = "forbid"`; `pedantic` + `nursery` + `cargo` lint groups enabled; `unwrap_used = "warn"` — prefer `expect()` with a message, or proper error handling.
- Naming: `snake_case` functions/modules, `CamelCase` types, `SCREAMING_SNAKE_CASE` constants.

## Testing

Use `cargo test` for the workspace, or narrow to a crate/module while iterating (for example `cargo test -p common physics`). Place unit tests next to the module under test, integration tests under `tests/`, and name tests descriptively (e.g. `test_player_collision_with_wall`).

## Commits & Pull Requests

- Short, imperative summaries.
- PRs: describe behavior impact on client/server, include repro steps or screenshots for client-facing changes, call out protocol or asset changes explicitly (they're breaking by default).

## Security & Assets

- `client/assets/` are not open source — replace before publishing a fork.
- `cert.pem` / `key.pem` are local-dev only. Do not commit production keys.
