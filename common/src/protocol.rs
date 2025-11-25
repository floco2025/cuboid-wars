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
#[derive(Debug, Clone, Copy, Component, PartialEq)]
#[cfg_attr(feature = "json", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "bincode", derive(Encode, Decode))]
pub struct Position {
    pub x: i32, // millimeters
    pub y: i32, // millimeters
}

// Velocity component - movement speed in units (millimeters) per second
#[derive(Debug, Clone, Copy, Component, Default)]
#[cfg_attr(feature = "json", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "bincode", derive(Encode, Decode))]
pub struct Velocity {
    pub x: f32,
    pub y: f32,
}

// Rotation component - yaw rotation in radians (used when stationary)
#[derive(Debug, Clone, Copy, Component, Default)]
#[cfg_attr(feature = "json", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "bincode", derive(Encode, Decode))]
pub struct Rotation {
    pub yaw: f32, // radians
}

// Player ID component - identifies which player an entity represents
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Component)]
#[cfg_attr(feature = "json", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "bincode", derive(Encode, Decode))]
pub struct PlayerId(pub u32);

// Kinematics - position, velocity, and rotation together
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "json", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "bincode", derive(Encode, Decode))]
pub struct Kinematics {
    pub pos: Position,
    pub vel: Velocity,
    pub rot: Rotation,
}

message! {
struct Player {
    pub kin: Kinematics,
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
// Client to Server: Velocity update.
struct CVelocity {
    pub vel: Velocity,
}
}

message! {
// Client to Server: Rotation update (for stationary players).
struct CRotation {
    pub rot: Rotation,
}
}

// ============================================================================
// Server Messages
// ============================================================================

message! {
// Server to Client: Initial server state after login.
struct SInit {
    pub id: PlayerId,
    pub player: Player,
    pub other_players: Vec<(PlayerId, Player)>,
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
// Server to Client: Player velocity update.
struct SVelocity {
    pub id: PlayerId,
    pub vel: Velocity,
}
}

message! {
// Server to Client: Player rotation update.
struct SRotation {
    pub id: PlayerId,
    pub rot: Rotation,
}
}

message! {
// Server to Client: Kinematics for all players.
struct SKinematics {
    pub kinematics: Vec<(PlayerId, Kinematics)>,
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
    Velocity(CVelocity),
    Rotation(CRotation),
}

// All server to client messages
#[derive(Debug, Clone, Message)]
#[cfg_attr(feature = "json", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "bincode", derive(Encode, Decode))]
pub enum ServerMessage {
    Init(SInit),
    Login(SLogin),
    Logoff(SLogoff),
    PlayerVelocity(SVelocity),
    PlayerRotation(SRotation),
    Kinematics(SKinematics),
}
