# Cuboid Wars

A fast-paced multiplayer arena game built with Rust, Bevy, Rapier, and QUIC.

![Cuboid Wars Screenshot](client/assets/screenshot1.png)

## Overview

Cuboid Wars is a networked 3D arena game on compact, multi-level maps. Players
move, jump, and shoot through corridors gated by color-coded barriers, fight
hostile actors (patrolling mines and drones that explode on death), and pick
up power-ups and cookies.

The game runs an authoritative server with client-side prediction, so movement
stays responsive while the server remains the source of truth for collisions,
items, projectiles, actor behaviour, and scoring.

> Hobby project — not currently accepting external contributions.

## Gameplay

- **Power-ups** — speed, multi-shot, phasing (passes through walls only), anti-gravity.
- **Barriers & keys** — coloured barriers block everyone; pick up a key of the
  matching colour to walk through that colour for the rest of your life.
- **Actors** — mines patrol and chase line-of-sight targets; killing them
  triggers a blast that can damage nearby players and actors.

## Controls

| Action | Key |
| --- | --- |
| Move | WASD |
| Sprint | hold Shift |
| Jump | Space |
| Look | mouse |
| Shoot | left click |
| Toggle cursor lock | Escape |
| Cycle camera view (first-person ↔ top-down) | V |
| Toggle level-focus (hide floors/walls on other levels) | R |
| Cycle debug colours | C |
| Toggle fullscreen | F11 / Ctrl-F / Cmd-F |

## Technical stack

- **Engine** — Bevy 0.18 (ECS)
- **Physics** — Rapier 0.32 (static map collision, kinematic characters, projectile shape casts)
- **Networking** — QUIC via `quinn`
- **Wire format** — `bincode` 2 (binary)
- **Architecture** — client–server with a shared `common` crate (protocol, physics, map types, spawn validation)

## Running locally

Cargo invocations default to `--release` in this repo (debug builds pull in too
much for our purposes).

```bash
cargo run --release --bin server                       # bind 127.0.0.1:8080
cargo run --release --bin client                       # connect to 127.0.0.1:8080
cargo run --release --bin client -- --name "Alice"     # custom name
```

For local multiplayer testing on macOS:

```bash
./launch_clients.sh 4              # 4 tiled windowed clients
./launch_clients.sh 2 100          # 2 clients with 100ms simulated lag
```

The repo ships a self-signed `cert.pem` / `key.pem` for LAN testing. **Replace
them for anything beyond localhost** — they are not production-safe.

## Map editor

```bash
python3 tools/editor.py            # edits config/server/map.json in place
```

The editor (PySide6) supports floors, walls, ramps, barriers, key/cookie/player
spawn zones, lights, and per-face material assignment.

## License

### Code

Dual-licensed under either:

- Apache License 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Assets

**The assets in `client/assets/` (3D models, textures, sounds, etc.) are NOT
open source.** They are licensed separately for use in this game only. If you
fork this repo you must replace all assets with your own or properly licensed
alternatives.
