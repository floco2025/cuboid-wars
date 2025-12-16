# Cuboid Wars

A fast-paced multiplayer arena shooter built with Rust, Bevy, and QUIC networking.

![Cuboid Wars Screenshot](client/assets/screenshot1.png)

## Overview

Cuboid Wars is a networked 3D game where players navigate a procedurally-generated maze, collect cookies for points, gather power-ups, and avoid ghosts. The game features a client-server architecture with authoritative server logic and client-side prediction for smooth gameplay.

## Technical Stack

- **Engine**: Bevy 0.17.3 (ECS game engine)
- **Networking**: QUIC protocol via quinn for low-latency multiplayer
- **Serialization**: bincode for efficient binary message encoding
- **Architecture**: Client-server with shared common crate

## License

### Code

The source code is dual-licensed under either:

* Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
* MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Assets

**The assets in the `client/assets/` directory (3D models, textures, sounds, etc.) are NOT open source.** They are licensed separately for use in this game only. If you fork or use this code, you must replace all assets with your own or properly licensed alternatives.