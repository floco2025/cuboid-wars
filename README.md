# Cuboid Wars

A fast-paced multiplayer arena shooter built with Rust, Bevy, and QUIC networking.

## Overview

Cuboid Wars is a networked 3D game where players navigate a procedurally-generated maze, collect cookies for points, gather power-ups, and avoid ghosts. The game features a client-server architecture with authoritative server logic and client-side prediction for smooth gameplay.

## Technical Stack

- **Engine**: Bevy 0.17.3 (ECS game engine)
- **Networking**: QUIC protocol via quinn for low-latency multiplayer
- **Serialization**: bincode for efficient binary message encoding
- **Architecture**: Client-server with shared common crate