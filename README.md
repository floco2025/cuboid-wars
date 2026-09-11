# Cuboid Wars

A fast-paced multiplayer arena game built with Rust, Bevy, Rapier, and renet.

![Cuboid Wars Screenshot](client/assets/screenshot1.png)
![Cuboid Wars Screenshot](client/assets/screenshot2.png)
![Cuboid Wars Screenshot](client/assets/screenshot3.png)
![Cuboid Wars Screenshot](client/assets/screenshot4.png)
![Cuboid Wars Screenshot](client/assets/screenshot5.png)


## Overview

Cuboid Wars is a networked 3D arena game on compact, multi-level maps.
Players run, jump, climb ladders, and shoot through corridors gated by
color-coded barriers, fight hostile scuttlers, bruisers, and zappers
that patrol and hunt, launch seeking missiles that fly the map's
airspace to their target, and complete quests for score.

Each client owns its player's movement; other clients interpolate its reports.
The server simulates actors; clients interpolate their movement samples.
The shooter decides bullet hits and simulates missiles; other clients simulate
cosmetic bullets and interpolate missiles. The server applies damage and manages
items, scoring, and the death/respawn flow.

## Gameplay

- **Humanoid robots** — animated players that walk, run, climb, jump, and land.
- **Quests** — objectives assigned at login, worth points when completed.
- **Gold** — collect gold coins for score and quest progress.
- **Power-ups** — single-shot, multi-shot, speed, low-gravity, and portal-gun pickups, plus instant-heal potions.
- **Seeking missiles** — collect a pack, lock onto a target, and fire; the
  missile flies the map's airspace to it.
- **Portal guns** — collect a gun to place linked portals and travel between them.
- **Equipment erasers** — walk-through energy fields that strip your weapons and power-ups.
- **Barriers & keys** — coloured barriers block everyone; the matching key
  lets you through until you die.
- **Light bridges** — ghostly walkways powered by pressure plates.
- **Pressure plates** — operate switches that open barriers, power bridges,
  run moving platforms, release guards, or launch fireworks, with
  configurable hold, toggle, and automatic solo/multiplayer behavior.
- **Actors** — scuttlers, bruisers, and zappers patrol and hunt; all
  explode when killed.
- **Turrets** — stationary guards with deadly sustained laser bursts.
- **Ladders** — climb between levels.
- **Moving maps** — tiles, rooms, and whole buildings that slide or lift through a map, everything inside riding along, monsters included. Get pinned by one and it kills you.
- **Fall damage** — short drops are safe; long falls scale up to lethal.
- **Checkpoints** — return to individual checkpoints or shared ones activated by any or all players.
- **Death & respawn** — return after a short delay, individually or with your group; some maps also restore enemies.
- **Scoring** — kills, gold, actor kills, and quest completions award
  points.
- **Weather & lighting** — rain and a bright/dim/dark light cycle, set per
  map.
- **Chat & admin console** — Enter to chat, `/` for commands; `/help` lists them and `/peace` toggles actor attacks.
- **One executable** — play alone, host a game your friends join, join theirs, or run a dedicated server.

## Controls

### Gameplay

| Action                                   | Key                  |
| ---------------------------------------- | -------------------- |
| Settings menu (also frees the cursor)    | Escape               |
| Look                                     | mouse                |
| Move                                     | WASD                 |
| Sprint                                   | hold Shift           |
| Jump                                     | Space                |
| Cycle weapons / multi-shot patterns      | Q                    |
| Fire selected weapon / portal A          | Left mouse button    |
| Place portal B (when both are available) | Right mouse button   |
| Zoom between first and third person      | mouse wheel          |
| Lock / unlock third-person camera        | F                    |
| Toggle fullscreen                        | F11 / Ctrl-F / Cmd-F |
| Chat                                     | Enter                |

Movement follows the camera. Unlocked, the robot faces where it walks; locked
with F or by zooming into first person, it faces the crosshair and can strafe.
Picking up a weapon selects it, and Q cycles through the ones you hold.

### Debug

| Action                                             | Key          |
| -------------------------------------------------- | ------------ |
| Toggle top-down view                               | V            |
| Toggle level-focus                                 | R            |
| Cycle bounds: off → collider+support → hitbox      | B            |
| Cycle debug colors: off → by material → by segment | C            |
| Release cursor                                     | Shift-Escape |
| Admin console                                      | /            |

## Technical stack

- **Engine** — Bevy (ECS)
- **Physics** — Rapier (static map collision, kinematic characters, projectile shape casts)
- **Networking** — UDP via `renet` and its netcode transport, polled from the game loop
- **Wire format** — `bincode` 2 (binary)
- **Architecture** — client–server with a shared `common` crate (protocol, physics, map types, spawn validation)

## Running locally

Cargo invocations default to `--release` in this repo (debug builds pull in too
much for our purposes).

```bash
cargo run --release                                    # single-player
cargo run --release -- --host                          # play and accept joiners on 127.0.0.1:8080
cargo run --release -- --host 0.0.0.0:8080             # accept joiners from the LAN
cargo run --release -- --join 192.168.1.100:8080 --name "Alice"
cargo run --release -- --serve --map hotel             # dedicated headless server on 127.0.0.1:8080
```

The server accepts anyone who can reach its port; keep it on a LAN you trust.

## Map editor

```bash
python3 tools/editor.py hotel      # edits config/server/maps/hotel/layout.json in place
```

Maps are listed by name in `config/server/gameplay.json` (`maps` + `default_map`). Each map has a folder containing `layout.json` and a hand-edited `settings.json` for movement, kind catalogs, respawn policies, quests, and other tuning. To add a map, register its name and create its settings file; the editor can then create its layout.

## License

### Code

Dual-licensed under either:

- Apache License 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Assets

**Assets in `client/assets/` are licensed separately from the source code.**
Project-created assets are provided for this game only; third-party assets retain
their own terms. See the [asset notice](client/assets/README.md) and
[provenance register](client/assets/ASSETS.md) for sources, licenses, and embedded dependencies.
