# Cuboid Wars

A fast-paced game built with Rust, Bevy, Rapier, and QUIC.

![Cuboid Wars Screenshot](client/assets/screenshot1.png)

## Overview

Cuboid Wars is a networked 3D game where players navigate a procedurally-generated multi-level maze, collect items, gather power-ups, and avoid sentries. The game uses shared Rapier-backed physics for player movement and projectiles, a client-server architecture with authoritative server logic, client-side prediction for smooth gameplay, and a small map editor for creating and refining arenas.

Client visuals are configured through the hand-edited JSON asset set at `client/assets/assets.json`, including material rules that use editor coordinates for floors and walls.

## Technical Stack

- **Engine**: Bevy 0.18 (ECS game engine)
- **Physics**: Rapier 0.32 for static map collision, kinematic character movement, and projectile shape casts
- **Networking**: QUIC protocol via quinn for low-latency multiplayer
- **Serialization**: bincode for efficient binary message encoding
- **Architecture**: Client-server with shared `common` crate for protocol, physics, map types, and spawn validation

## Development

```bash
cargo build
cargo test -p common physics
cargo run --bin server
cargo run --bin client
```

The server is authoritative for collisions, items, and map generation. The client performs local prediction using the same shared physics code and reconciles against server updates.

## License

### Code

The source code is dual-licensed under either:

* Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
* MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Assets

**The assets in the `client/assets/` directory (3D models, textures, sounds, etc.) are NOT open source.** They are licensed separately for use in this game only. If you fork or use this code, you must replace all assets with your own or properly licensed alternatives.
