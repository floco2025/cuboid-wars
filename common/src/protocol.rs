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

// Position component - i32 values represent millimeters for high precision
// Range: ±2,147,483 meters (±2,147 km) from origin
message! {
#[derive(Copy, Component, PartialEq)]
struct Position {
    pub x: i32, // millimeters
    pub y: i32, // millimeters
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

// ============================================================================
// Server Messages
// ============================================================================

message! {
// Server to Client: Initial connection acknowledgment with assigned player ID.
struct SInit {
    pub id: PlayerId,
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
// Server to Client: Periodic game state update for all players.
struct SUpdate {
    pub players: Vec<(PlayerId, Player)>,
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
    Update(SUpdate),
}
