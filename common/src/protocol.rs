#[cfg(feature = "json")]
use serde::{Deserialize, Serialize};

#[cfg(feature = "bincode")]
use bincode::{Decode, Encode};

use bevy_ecs::component::Component;
use bevy_ecs::message::Message;

// Macro to reduce boilerplate for structs
macro_rules! message {
    ($(#[$meta:meta])* struct $name:ident $body:tt) => {
        $(#[$meta])*
        #[derive(Debug, Clone)]
        #[cfg_attr(feature = "json", derive(Serialize, Deserialize))]
        #[cfg_attr(feature = "bincode", derive(Encode, Decode))]
        pub struct $name $body
    };
}

// ============================================================================
// Common Data Types
// ============================================================================

// Position component - 3D coordinates in meters (Bevy's coordinate system: X, Y=up, Z)
// Store as individual fields for network serialization (Vec3 doesn't implement bincode traits)
// Y is always 0 for now (2D gameplay on a flat plane)
message! {
#[derive(Copy, Component, PartialEq)]
struct Position {
    pub x: f32, // meters
    pub y: f32, // meters (up/down - always 0 for now)
    pub z: f32, // meters
}
}

// Velocity - movement speed state
#[derive(Debug, Copy, Clone, PartialEq, Default)]
#[cfg_attr(feature = "json", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "bincode", derive(Encode, Decode))]
pub enum Velocity {
    #[default]
    Idle,
    Walk,
    Run,
}

// Movement component - velocity state, movement direction, and facing direction
message! {
#[derive(Copy, Component, Default)]
struct Movement {
    pub vel: Velocity,
    pub move_dir: f32, // radians - direction of movement
    pub face_dir: f32, // radians - direction player is facing
}
}

// Player ID component - identifies which player an entity represents
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Component)]
#[cfg_attr(feature = "json", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "bincode", derive(Encode, Decode))]
pub struct PlayerId(pub u32);

// Player - complete player state
message! {
struct Player {
    pub pos: Position,
    pub mov: Movement,
    pub hits: i32,
}
}

// Wall orientation - horizontal (along X axis) or vertical (along Z axis)
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "bincode", derive(Encode, Decode))]
pub enum WallOrientation {
    Horizontal, // Along X axis
    Vertical,   // Along Z axis
}

// Wall - a wall segment on the grid
message! {
#[derive(Copy)]
struct Wall {
    pub x: f32,                     // Center X position
    pub z: f32,                     // Center Z position
    pub orientation: WallOrientation,
}
}

// ============================================================================
// Client Messages
// ============================================================================

message! {
// Client to Server: Login request.
struct CLogin {}
}

message! {
// Client to Server: Graceful disconnect notification.
struct CLogoff {}
}

message! {
// Client to Server: Movement update.
struct CMovement {
    pub mov: Movement,
}
}

message! {
// Client to Server: Shot fired.
struct CShot {
    pub mov: Movement,
}
}

message! {
// Client to Server: Echo request with timestamp.
struct CEcho {
    pub timestamp: u64,
}
}

// ============================================================================
// Server Messages
// ============================================================================

message! {
// Server to Client: Initial connection acknowledgment with assigned player ID.
struct SInit {
    pub id: PlayerId,
    pub walls: Vec<Wall>,
}
}

message! {
// Server to Client: Another player connected.
struct SLogin {
    pub id: PlayerId,
    pub player: Player,
}
}

message! {
// Server to Client: A player disconnected.
struct SLogoff {
    pub id: PlayerId,
    pub graceful: bool,
}
}

message! {
// Server to Client: Player movement update.
struct SMovement {
    pub id: PlayerId,
    pub mov: Movement,
}
}

message! {
// Server to Client: Player shot fired.
struct SShot {
    pub id: PlayerId,
    pub mov: Movement,
}
}

message! {
// Server to Client: Periodic game state update for all players.
struct SUpdate {
    pub players: Vec<(PlayerId, Player)>,
}
}

message! {
// Server to Client: Player was hit by a projectile.
struct SHit {
    pub id: PlayerId,        // Player who was hit
    pub hit_dir_x: f32,      // Direction of hit (normalized)
    pub hit_dir_z: f32,      // Direction of hit (normalized)
}
}

message! {
// Server to Client: Echo response with timestamp.
struct SEcho {
    pub timestamp: u64,
}
}

// ============================================================================
// Message Envelopes
// ============================================================================

// All client to server messages
#[derive(Debug, Clone)]
#[cfg_attr(feature = "json", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "bincode", derive(Encode, Decode))]
pub enum ClientMessage {
    Login(CLogin),
    Logoff(CLogoff),
    Movement(CMovement),
    Shot(CShot),
    Echo(CEcho),
}

// All server to client messages
#[derive(Debug, Clone, Message)]
#[cfg_attr(feature = "json", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "bincode", derive(Encode, Decode))]
pub enum ServerMessage {
    Init(SInit),
    Login(SLogin),
    Logoff(SLogoff),
    Movement(SMovement),
    Shot(SShot),
    Update(SUpdate),
    Hit(SHit),
    Echo(SEcho),
}
