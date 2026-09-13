# Cuboid Wars

A fast-paced multiplayer arena game built with Rust, Bevy, Rapier, and renet.

![Cuboid Wars Screenshot](client/assets/screenshot1.png)
![Cuboid Wars Screenshot](client/assets/screenshot2.png)
![Cuboid Wars Screenshot](client/assets/screenshot3.png)
![Cuboid Wars Screenshot](client/assets/screenshot4.png)
![Cuboid Wars Screenshot](client/assets/screenshot5.png)


## Overview

Cuboid Wars is a networked 3D game on multi-level maps, from combat arenas
to obstacle courses, built from the features below.

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

### Debug

| Action                                             | Key          |
| -------------------------------------------------- | ------------ |
| Cycle debug camera: level focus → all levels → off | V            |
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

For a new development machine, follow [the macOS and CachyOS setup instructions](DEPENDENCIES.md).

One executable plays alone, hosts a game your friends join, joins theirs, or
runs a dedicated server.

```bash
cargo run --release                                    # single-player
cargo run --release -- --god --peace                   # invincible players and peaceful enemies
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
