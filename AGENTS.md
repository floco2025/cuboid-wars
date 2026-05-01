# Repository Guidelines

This file is loaded by Claude Code, Codex, Cursor, and similar coding agents.

## Project Structure

Rust workspace with three crates:

- **`common/`** — shared between client and server.
  - `protocol.rs` — all `ClientMessage` / `ServerMessage` variants and wire structs (`Position`, `Speed`, `Player`, `MapLayout`, etc.).
  - `net.rs` — `MessageStream` abstraction over QUIC.
  - `physics/` — shared player/projectile movement, collision world, and spawn validation helpers.
  - `map.rs`, `types.rs`, `constants.rs` — shared map helpers, IDs, and gameplay constants.
- **`server/`** — authoritative headless server (Bevy `MinimalPlugins`).
  - `systems/` — players, projectiles, items, network broadcast.
  - `map/` — converts map definitions into runtime layout: cells/edges, floors, walls, ramps, lights, masks, and segments.
  - `assets/default.json` — default map source JSON.
  - Runs at 30 Hz via a manual `app.update()` loop.
- **`client/`** — Bevy renderer, input, UI.
  - `systems/network/` — consumes `ServerMessage`, spawns/updates entities.
  - `systems/players/` — local prediction, camera, rendering, movement feedback, and effects.
  - `systems/input/` — movement, shooting, and view/debug toggles.
  - `spawning/` — entity construction for players, projectiles, items, and map geometry.

Other notable paths:
- `tools/editor.py` — PySide6 map editor for `server/assets/default.json`.
- `tools/preview.py` — ASCII map preview/validation helper.
- `client/assets/` — 3D models, textures, audio.
- `client/assets/default.json` — hand-edited JSON asset set for materials, material rules, models, and sounds.
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
python3 tools/editor.py                           # edit server/assets/default.json
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
- The default map source is `server/assets/default.json`; the server turns it into `MapLayout`, sends that to clients, and both sides build shared collision/rendering state from it.
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
