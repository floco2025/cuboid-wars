use bevy_ecs::prelude::*;
use bincode::{Decode, Encode};

use crate::constants::{SPEED_RUN, SPEED_WALK};

// ============================================================================
// Common Data Types
// ============================================================================

// Position component - 3D coordinates in meters (Bevy's coordinate system: X, Y=up, Z).
// Stored as individual fields for serialization; Y varies based on ramps and roofs.
#[derive(Debug, Clone, Encode, Decode, Copy, Component, PartialEq, Default)]
pub struct Position {
    pub x: f32, // meters
    pub y: f32, // meters (up/down - elevation from ramps/roofs)
    pub z: f32, // meters
}

// SpeedLevel - discrete speed level.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default, Encode, Decode)]
pub enum SpeedLevel {
    #[default]
    Idle,
    Walk,
    Run,
}

// Speed component - speed level and direction.
#[derive(Debug, Clone, Encode, Decode, Copy, Component, Default)]
pub struct Speed {
    pub speed_level: SpeedLevel,
    pub move_dir: f32, // radians - direction of movement
}

impl Speed {
    #[must_use]
    pub fn to_velocity(&self) -> Velocity {
        let speed_magnitude = match self.speed_level {
            SpeedLevel::Idle => 0.0,
            SpeedLevel::Walk => SPEED_WALK,
            SpeedLevel::Run => SPEED_RUN,
        };
        Velocity {
            x: self.move_dir.sin() * speed_magnitude,
            y: 0.0,
            z: self.move_dir.cos() * speed_magnitude,
        }
    }
}

#[derive(Debug, Copy, Clone, Component, PartialEq, Default, Encode, Decode)]
pub struct Velocity {
    pub x: f32, // m/s
    pub y: f32, // m/s (up/down - always 0 for now)
    pub z: f32, // m/s
}

// Player ID component - identifies which player an entity represents.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Component, Encode, Decode)]
pub struct PlayerId(pub u32);

// Item ID component - identifies which item an entity represents.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Component, Encode, Decode)]
pub struct ItemId(pub u32);

// Ghost ID component - identifies which ghost an entity represents.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Component, Encode, Decode)]
pub struct GhostId(pub u32);

// FaceDirection component - direction player is facing (for rotation/aiming).
#[derive(Component, Default)]
pub struct FaceDirection(pub f32); // radians

// Player - complete player state snapshot sent across the network.
#[derive(Debug, Clone, Encode, Decode)]
pub struct Player {
    pub name: String,
    pub pos: Position,
    pub speed: Speed,
    pub face_dir: f32,
    pub hits: i32,
    pub speed_power_up: bool,
    pub multi_shot_power_up: bool,
    pub reflect_power_up: bool,
    pub phasing_power_up: bool,
    pub ghost_hunt_power_up: bool,
    pub stunned: bool,
}

// Wall - a wall segment on the grid.
#[derive(Debug, Clone, Encode, Decode, Copy)]
pub struct Wall {
    pub x1: f32,
    pub z1: f32,
    pub x2: f32,
    pub z2: f32,
    pub width: f32,
}

// Roof - a roof segment with corner coordinates.
#[derive(Debug, Clone, Encode, Decode, Copy)]
pub struct Roof {
    pub x1: f32,
    pub z1: f32,
    pub x2: f32,
    pub z2: f32,
    pub thickness: f32,
}

// Ramp - right triangular prism defined by low and high opposite corners
// Convention:
// - (x1, y1, z1) is on the floor at the low edge.
// - (x2, y2, z2) is on the roof at the opposite corner (high edge).
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

// Precomputed wall light placement sent from server to client.
#[derive(Debug, Clone, Encode, Decode, Copy)]
pub struct WallLight {
    pub model_pos: Position,
    pub light_pos: Position,
    pub yaw: f32,
}

// Full grid configuration sent once on connect.
#[derive(Debug, Clone, Encode, Decode, Resource)]
pub struct MapLayout {
    pub boundary_walls: Vec<Wall>,
    pub interior_walls: Vec<Wall>,
    pub lower_walls: Vec<Wall>, // Boundary walls + interior walls
    pub roof_walls: Vec<Wall>,  // Invisible roof walls for collision
    pub roofs: Vec<Roof>,
    pub ramps: Vec<Ramp>,
    pub wall_lights: Vec<WallLight>,
}

// Item type - different types of items.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Encode, Decode)]
pub enum ItemType {
    SpeedPowerUp,
    MultiShotPowerUp,
    ReflectPowerUp,
    PhasingPowerUp,
    GhostHuntPowerUp,
    Cookie,
}

// Item - an item on the map.
#[derive(Debug, Clone, Encode, Decode, Copy)]
pub struct Item {
    pub item_type: ItemType,
    pub pos: Position,
}

// Ghost - a ghost moving around the map.
#[derive(Debug, Clone, Encode, Decode, Copy)]
pub struct Ghost {
    pub pos: Position,
    pub vel: Velocity,
}

// ============================================================================
// Client Messages
// ============================================================================

// Client to Server: Login request.
#[derive(Debug, Clone, Encode, Decode)]
pub struct CLogin {
    pub name: String,
}

// Client to Server: Graceful disconnect notification.
#[derive(Debug, Clone, Encode, Decode)]
pub struct CLogoff {}

// Client to Server: Speed update.
#[derive(Debug, Clone, Encode, Decode)]
pub struct CSpeed {
    pub speed: Speed,
}

// Client to Server: Facing direction update.
#[derive(Debug, Clone, Encode, Decode)]
pub struct CFace {
    pub dir: f32, // radians - direction player is facing
}

// Client to Server: Shot fired.
#[derive(Debug, Clone, Encode, Decode)]
pub struct CShot {
    pub face_dir: f32,   // radians - yaw direction player is facing when shooting
    pub face_pitch: f32, // radians - pitch (up/down) when shooting
}

// Client to Server: Echo request with timestamp (Duration since app start, serialized as nanoseconds).
#[derive(Debug, Clone, Encode, Decode)]
pub struct CEcho {
    pub timestamp_nanos: u64,
}

// ============================================================================
// Server Messages
// ============================================================================

// Server to Client: Initial connection acknowledgment with assigned player ID.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SInit {
    pub id: PlayerId,
    pub grid_config: MapLayout,
}

// Server to Client: Another player connected.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SLogin {
    pub id: PlayerId,
    pub player: Player,
}

// Server to Client: A player disconnected.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SLogoff {
    pub id: PlayerId,
    pub graceful: bool,
}

// Server to Client: Player speed update with position for reconciliation.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SSpeed {
    pub id: PlayerId,
    pub speed: Speed,
    pub pos: Position,
}

// Server to Client: Player facing direction update.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SFace {
    pub id: PlayerId,
    pub dir: f32, // radians - direction player is facing
}

// Server to Client: Player shot fired.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SShot {
    pub id: PlayerId,
    pub face_dir: f32,   // radians - yaw direction player is facing when shooting
    pub face_pitch: f32, // radians - pitch (up/down) when shooting
}

// Server to Client: Periodic game state update for all players.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SUpdate {
    pub seq: u32,
    pub players: Vec<(PlayerId, Player)>,
    pub items: Vec<(ItemId, Item)>,
    pub ghosts: Vec<(GhostId, Ghost)>,
}

// Server to Client: Player was hit by a projectile.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SHit {
    pub id: PlayerId,   // Player who was hit
    pub hit_dir_x: f32, // Direction of hit (normalized)
    pub hit_dir_z: f32, // Direction of hit (normalized)
}

// Server to Client: Player status effects changed.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SPlayerStatus {
    pub id: PlayerId,
    pub speed_power_up: bool,
    pub multi_shot_power_up: bool,
    pub reflect_power_up: bool,
    pub phasing_power_up: bool,
    pub ghost_hunt_power_up: bool,
    pub stunned: bool,
}

// Server to Client: Echo response.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SEcho {
    pub timestamp_nanos: u64,
}

// Server to Client: Ghost direction changed.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SGhost {
    pub id: GhostId,
    pub ghost: Ghost,
}

// Server to Client: Player collected a cookie.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SCookieCollected {}

// Server to Client: Ghost hit a player.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SGhostHit {}

// ============================================================================
// Message Envelopes
// ============================================================================

// All client to server messages
#[derive(Debug, Clone, Encode, Decode)]
pub enum ClientMessage {
    Login(CLogin),
    Logoff(CLogoff),
    Speed(CSpeed),
    Face(CFace),
    Shot(CShot),
    Echo(CEcho),
}

// All server to client messages
#[derive(Debug, Clone, Message, Encode, Decode)]
pub enum ServerMessage {
    Init(SInit),
    Login(SLogin),
    Logoff(SLogoff),
    Speed(SSpeed),
    Face(SFace),
    Shot(SShot),
    Update(SUpdate),
    Hit(SHit),
    PlayerStatus(SPlayerStatus),
    Echo(SEcho),
    Ghost(SGhost),
    CookieCollected(SCookieCollected),
    GhostHit(SGhostHit),
}
