#[cfg(feature = "json")]
use serde::{Deserialize, Serialize};

#[cfg(feature = "bincode")]
use bincode::{Decode, Encode};

use bevy_ecs::component::Component;

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

/// Position component - used by both client and server ECS
#[derive(Debug, Clone, Copy, Component)]
#[cfg_attr(feature = "json", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "bincode", derive(Encode, Decode))]
pub struct Position {
    pub x: i32,
    pub y: i32,
}

/// Player ID component - identifies which player an entity represents
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Component)]
#[cfg_attr(feature = "json", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "bincode", derive(Encode, Decode))]
pub struct PlayerId(pub u32);

message! {
struct Player {
    pub pos: Position,
}
}

// ============================================================================
// Client Messages
// ============================================================================

message! {
/// Client to Server: Login request.
struct CLogin {}
}

message! {
/// Client to Server: Graceful disconnect notification.
struct CLogoff {}
}

// ============================================================================
// Server Messages
// ============================================================================

message! {
/// Server to Client: Initial server state after login.
struct SInit {
    pub id: PlayerId,                      // The id that the server uses for the client
    pub players: Vec<(PlayerId, Player)>, // All player ids and their data
}
}

message! {
/// Server to Client: Another player connected.
struct SLogin {
    pub id: PlayerId,   // The id for the new player
    pub player: Player, // The new player
}
}

message! {
/// Server to Client: A player disconnected.
struct SLogoff {
    pub id: PlayerId,   // The id of the player who disconnected
    pub graceful: bool, // Graceful disconnect if true
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
}

// All server to client messages
#[derive(Debug, Clone)]
#[cfg_attr(feature = "json", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "bincode", derive(Encode, Decode))]
pub enum ServerMessage {
    Init(SInit),
    Login(SLogin),
    Logoff(SLogoff),
}
