use std::ops::AddAssign;

use bevy_ecs::prelude::*;
use bevy_math::Vec3;
use bincode::{Decode, Encode};

use crate::constants::PLAYER_SPEED;

// ============================================================================
// Components
// ============================================================================

// Position component - 3D coordinates in meters (Bevy's coordinate system: X, Y=up, Z).
// Stored as individual fields for serialization; Y varies based on ramps and floors.
#[derive(Debug, Clone, Encode, Decode, Copy, Component, PartialEq, Default)]
pub struct Position {
    pub x: f32, // meters
    pub y: f32, // meters (up/down - elevation from ramps/floors)
    pub z: f32, // meters
}

impl From<Vec3> for Position {
    fn from(v: Vec3) -> Self {
        Self { x: v.x, y: v.y, z: v.z }
    }
}

impl From<Position> for Vec3 {
    fn from(p: Position) -> Self {
        Self::new(p.x, p.y, p.z)
    }
}

impl AddAssign<Vec3> for Position {
    fn add_assign(&mut self, rhs: Vec3) {
        self.x += rhs.x;
        self.y += rhs.y;
        self.z += rhs.z;
    }
}

// CharacterMoveIntent component - desired character movement. `Idle` while
// standing still, `Moving { direction }` while moving (radians, world-space).
// The horizontal velocity is derived each tick from this plus character rules.
#[derive(Debug, Clone, Encode, Decode, Copy, Component, Default, PartialEq)]
pub enum CharacterMoveIntent {
    #[default]
    Idle,
    Moving {
        direction: f32,
    },
}

impl CharacterMoveIntent {
    // Movement direction (radians) if active, else `None`.
    #[must_use]
    pub const fn direction(&self) -> Option<f32> {
        match self {
            Self::Idle => None,
            Self::Moving { direction } => Some(*direction),
        }
    }

    // Convert intent to a horizontal world-space velocity at the given magnitude.
    #[must_use]
    pub fn to_horizontal_velocity(&self, magnitude: f32) -> Vec3 {
        match self {
            Self::Idle => Vec3::ZERO,
            Self::Moving { direction } => Vec3::new(direction.sin() * magnitude, 0.0, direction.cos() * magnitude),
        }
    }

    // Horizontal velocity for a player with the speed power-up multiplier applied.
    #[must_use]
    pub fn to_player_horizontal_velocity(&self, has_speed_power_up: bool) -> Vec3 {
        let mag = if has_speed_power_up {
            PLAYER_SPEED * crate::constants::POWER_UP_SPEED_MULTIPLIER
        } else {
            PLAYER_SPEED
        };
        self.to_horizontal_velocity(mag)
    }
}

// Player ID component - identifies which player an entity represents.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Component, Encode, Decode)]
pub struct PlayerId(pub u32);

// Actor ID component - identifies a server-controlled non-player character.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Component, Encode, Decode)]
pub struct ActorId(pub u32);

// Item ID component - identifies which item an entity represents.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Component, Encode, Decode)]
pub struct ItemId(pub u32);

// FaceDirection component - direction player is facing (for rotation/aiming).
#[derive(Component, Default)]
pub struct FaceDirection(pub f32); // radians

// ============================================================================
// Geometry
// ============================================================================

// Wall - a wall segment on the grid.
#[derive(Debug, Clone, Encode, Decode, Copy)]
pub struct Wall {
    pub x1: f32,
    pub z1: f32,
    pub x2: f32,
    pub z2: f32,
    pub width: f32,
    pub level: u8,
}

impl Wall {
    // Returns `(min_x, max_x, min_z, max_z)` bounds for this wall.
    #[must_use]
    pub const fn bounds_xz(&self) -> (f32, f32, f32, f32) {
        (
            self.x1.min(self.x2),
            self.x1.max(self.x2),
            self.z1.min(self.z2),
            self.z1.max(self.z2),
        )
    }
}

// Floor - a horizontal slab at some level (level 0 = ground). `y` is
// the top (standing) surface; the slab extends down to `y - thickness`.
#[derive(Debug, Clone, Encode, Decode, Copy)]
pub struct Floor {
    pub x1: f32,
    pub z1: f32,
    pub x2: f32,
    pub z2: f32,
    pub y: f32,
    pub thickness: f32,
    pub level: u8,
}

impl Floor {
    // Returns `(min_x, max_x, min_z, max_z)` bounds for this floor.
    #[must_use]
    pub const fn bounds_xz(&self) -> (f32, f32, f32, f32) {
        (
            self.x1.min(self.x2),
            self.x1.max(self.x2),
            self.z1.min(self.z2),
            self.z1.max(self.z2),
        )
    }
}

// Ramp - right triangular prism defined by low and high opposite corners
// Convention:
// - (x1, y1, z1) is on the lower floor at the low edge.
// - (x2, y2, z2) is on the upper floor at the opposite corner (high edge).
// - Footprint is the axis-aligned rectangle spanned by (x1, z1) and (x2, z2).
// - Slope runs from the low edge to the high edge across that rectangle.
#[derive(Debug, Clone, Encode, Decode, Copy)]
pub struct Ramp {
    pub x1: f32,
    pub y1: f32,
    pub z1: f32,
    pub x2: f32,
    pub y2: f32,
    pub z2: f32,
}

impl Ramp {
    // Returns `(min_x, max_x, min_z, max_z)` bounds for this ramp's footprint.
    #[must_use]
    pub const fn bounds_xz(&self) -> (f32, f32, f32, f32) {
        (
            self.x1.min(self.x2),
            self.x1.max(self.x2),
            self.z1.min(self.z2),
            self.z1.max(self.z2),
        )
    }

    // Returns `(min_y, max_y)` height bounds for this ramp.
    #[must_use]
    pub const fn bounds_y(&self) -> (f32, f32) {
        (self.y1.min(self.y2), self.y1.max(self.y2))
    }
}

// Precomputed wall light placement sent from server to client.
#[derive(Debug, Clone, Encode, Decode, Copy)]
pub struct WallLight {
    pub pos: Position,
    pub yaw: f32,
}

// ============================================================================
// Game Data
// ============================================================================

// Item type - different types of items.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Encode, Decode)]
pub enum ItemType {
    SpeedPowerUp,
    MultiShotPowerUp,
    PhasingPowerUp,
    Cookie,
}

// Character movement state needed to continue client-side prediction from an
// authoritative server point.
#[derive(Debug, Clone, Copy, Encode, Decode)]
pub struct CharacterMovementState {
    pub pos: Position,
    pub move_intent: CharacterMoveIntent,
    pub vertical_velocity: f32, // m/s, negative = falling
}

impl CharacterMovementState {
    #[must_use]
    pub const fn new(pos: Position, move_intent: CharacterMoveIntent, vertical_velocity: f32) -> Self {
        Self {
            pos,
            move_intent,
            vertical_velocity,
        }
    }
}

// Server-controlled actor category. Starts narrow, but keeps snapshots ready
// for different actor models and behavior later.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum ActorKind {
    Automaton,
}

// Actor - complete server-controlled actor snapshot sent across the network.
#[derive(Debug, Clone, Encode, Decode)]
pub struct Actor {
    pub kind: ActorKind,
    pub movement: CharacterMovementState,
    pub face_dir: f32,
}

impl Actor {
    #[must_use]
    pub const fn new(kind: ActorKind, pos: Position, move_intent: CharacterMoveIntent, face_dir: f32) -> Self {
        Self {
            kind,
            movement: CharacterMovementState::new(pos, move_intent, 0.0),
            face_dir,
        }
    }
}

// Player - complete player state snapshot sent across the network.
#[derive(Debug, Clone, Encode, Decode)]
pub struct Player {
    pub name: String,
    pub movement: CharacterMovementState,
    pub face_dir: f32,
    pub hits: i32,
    pub speed_power_up: bool,
    pub multi_shot_power_up: bool,
    pub phasing_power_up: bool,
    pub stunned: bool,
}

impl Player {
    // Creates a new player with the given core fields and all status flags set to `false`.
    #[must_use]
    pub const fn new(name: String, pos: Position, move_intent: CharacterMoveIntent, face_dir: f32, hits: i32) -> Self {
        Self {
            name,
            movement: CharacterMovementState::new(pos, move_intent, 0.0),
            face_dir,
            hits,
            speed_power_up: false,
            multi_shot_power_up: false,
            phasing_power_up: false,
            stunned: false,
        }
    }
}

// Item - an item on the map.
#[derive(Debug, Clone, Encode, Decode, Copy)]
pub struct Item {
    pub item_type: ItemType,
    pub pos: Position,
}

// Full grid configuration sent once on connect.
#[derive(Debug, Clone, Encode, Decode, Resource)]
pub struct MapLayout {
    pub walls: Vec<Wall>,
    pub ramps: Vec<Ramp>,
    pub floors: Vec<Floor>,
    pub wall_lights: Vec<WallLight>,
}
