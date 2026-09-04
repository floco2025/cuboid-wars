# Cuboid Wars

A fast-paced multiplayer arena game built with Rust, Bevy, Rapier, and QUIC.

![Cuboid Wars Screenshot](client/assets/screenshot1.png)
![Cuboid Wars Screenshot](client/assets/screenshot2.png)
![Cuboid Wars Screenshot](client/assets/screenshot3.png)
![Cuboid Wars Screenshot](client/assets/screenshot4.png)
![Cuboid Wars Screenshot](client/assets/screenshot5.png)


## Overview

Cuboid Wars is a networked 3D arena game on compact, multi-level maps.
Players run, jump, climb ladders, and shoot through corridors gated by
color-coded barriers, fight the hostile mines, sentries, zappers, and
reapers that patrol and hunt, launch seeking missiles that fly the map's
airspace to their target, and complete quests for score.

The game runs an authoritative server with client-side prediction, so
movement stays responsive while the server remains the source of truth
for collisions, items, projectiles, actor behaviour, scoring, and the
death/respawn flow.

## Gameplay

- **Quests** — objectives assigned at login, worth points when completed.
- **Cookies** — scattered pickups worth score and quest progress.
- **Power-ups** — timed boosts (speed, multi-shot, low-gravity) and
  instant-heal potions.
- **Seeking missiles** — collect a pack, lock onto a target, and fire; the
  missile flies the map's airspace to it.
- **Portal gun** — shoot portals onto any surface and step through one to
  come out the other; a map hands you both ends or one end shared with a
  partner, and playing alone you always get both.
- **Barriers & keys** — coloured barriers block everyone; the matching key
  lets you through until you die.
- **Light bridges** — ghostly walkways that turn solid while their plates are held.
- **Pressure plates** — some barrier colours open for everyone while
  enough players stand on their plates; alone, each plate is a switch.
- **Actors** — mines, sentries, zappers, and reapers patrol and hunt; all
  explode when killed.
- **Ladders** — climb between levels.
- **Fall damage** — short drops are safe; long falls scale up to lethal.
- **Death & respawn** — dying drops your keys and ammo; you respawn after
  a short delay.
- **Scoring** — kills, cookies, actor kills, and quest completions award
  points.
- **Weather & lighting** — rain and a bright/dim/dark light cycle, set per
  map.
- **Chat & admin console** — Enter to chat, `/` for commands; `/help` lists them.

## Controls

| Action | Key |
| --- | --- |
| Move | WASD |
| Sprint | hold Shift |
| Jump | Space |
| Climb ladder | walk into it (Space lets go) |
| Look | mouse |
| Cycle weapons / multi-shot patterns | Q |
| Fire selected weapon / portal A | Left mouse button |
| Place portal B (when both portals are available) | Right mouse button |
| Chat / admin console | Enter or `/` (↑/↓ history) |
| Settings menu (also frees the cursor) | Escape |
| Cycle camera view (first-person ↔ top-down) | V |
| Toggle level-focus (hide floors/walls on other levels) | R |
| Toggle fullscreen | F11 / Ctrl-F / Cmd-F |

## Technical stack

- **Engine** — Bevy (ECS)
- **Physics** — Rapier (static map collision, kinematic characters, projectile shape casts)
- **Networking** — QUIC via `quinn`
- **Wire format** — `bincode` 2 (binary)
- **Architecture** — client–server with a shared `common` crate (protocol, physics, map types, spawn validation)

## Running locally

Cargo invocations default to `--release` in this repo (debug builds pull in too
much for our purposes).

```bash
cargo run --release --bin server                       # bind 127.0.0.1:8080, loads default_map
cargo run --release --bin server -- --map hotel        # load a specific map
cargo run --release --bin client                       # connect to 127.0.0.1:8080
cargo run --release --bin client -- --name "Alice"     # custom name
```

For local multiplayer testing on macOS:

```bash
./launch_clients.sh 4              # 4 tiled windowed clients
./launch_clients.sh 2 100          # 2 clients with 100ms simulated lag each way
./launch_clients.sh 2 100 0.1      # ... and 10% of unreliable messages dropped
```

The repo ships a self-signed `cert.pem` / `key.pem` for LAN testing. **Replace
them for anything beyond localhost** — they are not production-safe.

## Map editor

```bash
python3 tools/editor.py hotel      # edits config/server/maps/hotel.json in place
```

Maps are registered in `config/server/gameplay.json` (`maps` + `default_map`).
The editor (PySide6) covers everything in a map file: floors, grass, walls,
ramps, ladders, barriers, spawn zones, items, pressure plates, lights, and
per-face materials.

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
