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

- **Quests** — every player gets the quest list at login; completing one
  flashes its banner and pays out points.
- **Cookies** — scattered pickups worth score and quest progress.
- **Power-ups** — timed boosts: speed, multi-shot, and low-gravity.
  Health potions heal instantly.
- **Seeking missiles** — grab missile packs, lock onto a player or
  actor, and press F; the missile hunts its target through the map.
- **Barriers & keys** — coloured barriers block everyone; the matching
  key lets you pass for the rest of your current life.
- **Pressure plates** — some barrier colours open for everyone while
  enough players stand on their plates.
- **Actors** — mines and sentries patrol and chase; zappers keep their
  distance and fire tracking lasers; reapers chase and fire lasers too.
  All of them explode when killed.
- **Ladders** — climb between levels: push toward a ladder to grab it,
  jump to let go.
- **Fall damage** — short drops are safe; long falls scale up to lethal.
- **Death & respawn** — dying drops your keys and per-life gear; you
  respawn at a spawn zone after a short delay.
- **Scoring** — kills, deaths, cookies, actor hits and bounties, and
  quest completions all award tunable point values.
- **Weather & lighting** — rain and a bright/dim/dark light cycle run
  server-side; each map holds a fixed state or follows the automatic
  cycles.
- **Chat & admin console** — Enter to chat, `/` for commands; `/help` lists them.

## Controls

| Action | Key |
| --- | --- |
| Move | WASD |
| Sprint | hold Shift |
| Jump | Space |
| Look | mouse |
| Shoot | left click |
| Fire missile (needs lock-on) | F |
| Chat / admin console | Enter or `/` (↑/↓ history) |
| Toggle cursor lock | Escape |
| Cycle camera view (first-person ↔ top-down) | V |
| Toggle level-focus (hide floors/walls on other levels) | R |
| Cycle debug colours | C |
| Toggle fullscreen | F11 / Ctrl-F / Cmd-F |

## Technical stack

- **Engine** — Bevy 0.19 (ECS)
- **Physics** — Rapier 0.32 (static map collision, kinematic characters, projectile shape casts)
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
./launch_clients.sh 2 100          # 2 clients with 100ms simulated lag
```

The repo ships a self-signed `cert.pem` / `key.pem` for LAN testing. **Replace
them for anything beyond localhost** — they are not production-safe.

## Map editor

```bash
python3 tools/editor.py hotel      # edits config/server/maps/hotel.json in place
```

Maps are registered in `config/server/gameplay.json` (`maps` + `default_map`);
each entry sets the map's skybox, gravity values, and weather/lighting modes.
Passing a new name opens an empty map and Save creates its file — add a
registry entry to make the server load it.

The editor (PySide6) supports floors, grass, walls, ramps, ladders,
barriers, actor/player spawn zones, placed items (power-ups, health
potions, cookies, keys, missile packs), pressure plates, lights, and
per-face material assignment.

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
