use bevy_ecs::component::Component;
use bevy_ecs::message::Message;
#[cfg(feature = "bincode")]
use bincode::{Decode, Encode};
#[cfg(feature = "json")]
use serde::{Deserialize, Serialize};

use crate::constants::{RUN_SPEED, WALK_SPEED};

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

// Position component - 3D coordinates in meters (Bevy's coordinate system: X, Y=up, Z).
// Stored as individual fields for serialization; Y is effectively 0 for the flat arena.
message! {
#[derive(Copy, Component, PartialEq, Default)]
struct Position {
    pub x: f32, // meters
    pub y: f32, // meters (up/down - always 0 for now)
    pub z: f32, // meters
}
}

// SpeedLevel - discrete speed level.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "json", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "bincode", derive(Encode, Decode))]
pub enum SpeedLevel {
    #[default]
    Idle,
    Walk,
    Run,
}

// Speed component - speed level and direction.
message! {
#[derive(Copy, Component, Default)]
struct Speed {
    pub speed_level: SpeedLevel,
    pub move_dir: f32, // radians - direction of movement
}
}

impl Speed {
    #[must_use]
    pub fn to_velocity(&self) -> Velocity {
        let speed_magnitude = match self.speed_level {
            SpeedLevel::Idle => 0.0,
            SpeedLevel::Walk => WALK_SPEED,
            SpeedLevel::Run => RUN_SPEED,
        };
        Velocity {
            x: self.move_dir.sin() * speed_magnitude,
            y: 0.0,
            z: self.move_dir.cos() * speed_magnitude,
        }
    }
}

#[derive(Copy, Clone, Component, PartialEq, Default)]
#[cfg_attr(feature = "json", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "bincode", derive(Encode, Decode))]
pub struct Velocity {
    pub x: f32, // m/s
    pub y: f32, // m/s (up/down - always 0 for now)
    pub z: f32, // m/s
}

// Player ID component - identifies which player an entity represents.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Component)]
#[cfg_attr(feature = "json", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "bincode", derive(Encode, Decode))]
pub struct PlayerId(pub u32);

// Item ID component - identifies which item an entity represents.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Component)]
#[cfg_attr(feature = "json", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "bincode", derive(Encode, Decode))]
pub struct ItemId(pub u32);

// FaceDirection component - direction player is facing (for rotation/aiming).
#[derive(Component, Default)]
pub struct FaceDirection(pub f32); // radians

// Player - complete player state snapshot sent across the network.
message! {
struct Player {
    pub name: String,
    pub pos: Position,
    pub speed: Speed,
    pub face_dir: f32,
    pub hits: i32,
    pub speed_power_up: bool,
    pub multi_shot_power_up: bool,
}
}

// Wall orientation - horizontal (along X axis) or vertical (along Z axis).
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "bincode", derive(Encode, Decode))]
pub enum WallOrientation {
    Horizontal, // Along X axis
    Vertical,   // Along Z axis
}

// Wall - a wall segment on the grid.
message! {
#[derive(Copy)]
struct Wall {
    pub x: f32,                     // Center X position
    pub z: f32,                     // Center Z position
    pub orientation: WallOrientation,
}
}

// Roof - a roof covering a grid cell.
message! {
#[derive(Copy)]
struct Roof {
    pub row: u32,  // Grid row
    pub col: u32,  // Grid column
}
}

// Item type - different types of items.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "bincode", derive(Encode, Decode))]
pub enum ItemType {
    SpeedPowerUp,
    MultiShotPowerUp,
}

// Item - an item on the map.
message! {
#[derive(Copy)]
struct Item {
    pub item_type: ItemType,
    pub pos: Position,
}
}

// ============================================================================
// Client Messages
// ============================================================================

message! {
// Client to Server: Login request.
struct CLogin {
    pub name: String
}
}

message! {
// Client to Server: Graceful disconnect notification.
struct CLogoff {}
}

message! {
// Client to Server: Speed update.
struct CSpeed {
    pub speed: Speed,
}
}

message! {
// Client to Server: Facing direction update.
struct CFace {
    pub dir: f32, // radians - direction player is facing
}
}

message! {
// Client to Server: Shot fired.
struct CShot {
    pub face_dir: f32, // radians - direction player is facing when shooting
}
}

message! {
// Client to Server: Echo request with timestamp (Duration since app start, serialized as nanoseconds).
struct CEcho {
    pub timestamp_nanos: u64,
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
    pub roofs: Vec<Roof>,
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
// Server to Client: Player speed update.
struct SSpeed {
    pub id: PlayerId,
    pub speed: Speed,
}
}

message! {
// Server to Client: Player facing direction update.
struct SFace {
    pub id: PlayerId,
    pub dir: f32, // radians - direction player is facing
}
}

message! {
// Server to Client: Player shot fired.
struct SShot {
    pub id: PlayerId,
    pub face_dir: f32, // radians - direction player is facing when shooting
}
}

message! {
// Server to Client: Periodic game state update for all players.
struct SUpdate {
    pub seq: u32,
    pub players: Vec<(PlayerId, Player)>,
    pub items: Vec<(ItemId, Item)>,
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
// Server to Client: Player power-up status changed.
struct SPowerUp {
    pub id: PlayerId,
    pub speed_power_up: bool,
    pub multi_shot_power_up: bool,
}
}

message! {
// Server to Client: Echo response.
struct SEcho {
    pub timestamp_nanos: u64,
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
    Speed(CSpeed),
    Face(CFace),
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
    Speed(SSpeed),
    Face(SFace),
    Shot(SShot),
    Update(SUpdate),
    Hit(SHit),
    PowerUp(SPowerUp),
    Echo(SEcho),
}
