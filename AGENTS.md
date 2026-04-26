# Repository Guidelines

This file is loaded by Claude Code, Codex, Cursor, and similar coding agents.

## Project Structure

Rust workspace with three crates:

- **`common/`** — shared between client and server.
  - `protocol.rs` — all `ClientMessage` / `ServerMessage` variants and wire structs (`Position`, `Speed`, `Player`, `MapLayout`, etc.).
  - `net.rs` — `MessageStream` abstraction over QUIC.
  - `physics/` — kinematic state (`PlayerMotion`, `ProjectileMotion`), gravity/drag integration, collision detection (sweeps/overlaps), and collision response (sliding/bouncing).
- **`server/`** — authoritative headless server (Bevy `MinimalPlugins`).
  - `systems/` — players, projectiles, items, network broadcast.
  - `map/` — procedural maze generation (walls, roofs, ramps).
  - Runs at 30 Hz via a manual `app.update()` loop.
- **`client/`** — Bevy renderer, input, UI.
  - `systems/network/` — consumes `ServerMessage`, spawns/updates entities.
  - `systems/players/` — local prediction, camera, effects.
  - `spawning/` — entity construction (players, items, map geometry).

Other notable paths:
- `client/assets/` — 3D models, textures, audio.
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
cargo clippy                                      # pedantic + nursery + cargo lint groups
cargo fmt
```

## Architecture Notes

**Server is authoritative for**: player positions (replies with `SSpeed` carrying a reconciliation position), all collisions, item spawning/collection, map generation (sent once via `SInit` on connect).

**Client owns**: input, local movement prediction, rendering, camera, UI.

**Conventions**:
- Entity IDs are newtype wrappers: `PlayerId(u32)`, `ItemId(u32)`.
- Bevy resources `PlayerMap` / `ItemMap` map IDs to entities (server- and client-side).
- Tokio mpsc channels bridge async QUIC I/O with Bevy's sync systems.
- Coordinates: Bevy Y-up `(x, y, z)`, units in meters.
- Wire format: `bincode` 2 (binary).

## Coding Style

- Rust edition 2024. Format with `cargo fmt` (see `rustfmt.toml`).
- Workspace lints (root `Cargo.toml`): `unsafe_code = "forbid"`; `pedantic` + `nursery` + `cargo` lint groups enabled; `unwrap_used = "warn"` — prefer `expect()` with a message, or proper error handling.
- Naming: `snake_case` functions/modules, `CamelCase` types, `SCREAMING_SNAKE_CASE` constants.

## Testing

No automated tests currently. If adding them, use `cargo test`, place unit tests next to the module under test, integration tests under `tests/`. Name tests descriptively (e.g. `test_player_collision_with_wall`).

## Commits & Pull Requests

- Short, imperative summaries.
- PRs: describe behavior impact on client/server, include repro steps or screenshots for client-facing changes, call out protocol or asset changes explicitly (they're breaking by default).

## Security & Assets

- `client/assets/` are not open source — replace before publishing a fork.
- `cert.pem` / `key.pem` are local-dev only. Do not commit production keys.
