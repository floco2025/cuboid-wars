use bevy_ecs::prelude::*;
use bincode::{Decode, Encode};

use crate::constants::{FIELD_DEPTH, FIELD_WIDTH, GRID_COLS, GRID_ROWS, GRID_SIZE, SPEED_RUN, SPEED_WALK};

// Macro to reduce boilerplate for structs
macro_rules! message {
    ($(#[$meta:meta])* struct $name:ident $body:tt) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Encode, Decode)]
        pub struct $name $body
    };
}

// ============================================================================
// Common Data Types
// ============================================================================

// Position component - 3D coordinates in meters (Bevy's coordinate system: X, Y=up, Z).
// Stored as individual fields for serialization; Y varies based on ramps and roofs.
message! {
#[derive(Copy, Component, PartialEq, Default)]
struct Position {
    pub x: f32, // meters
    pub y: f32, // meters (up/down - elevation from ramps/roofs)
    pub z: f32, // meters
}
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
message! {
struct Player {
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
}

// Wall - a wall segment on the grid.
message! {
#[derive(Copy)]
struct Wall {
    pub x1: f32,
    pub z1: f32,
    pub x2: f32,
    pub z2: f32,
    pub width: f32,
}
}

// Roof - a roof segment with corner coordinates.
message! {
#[derive(Copy)]
struct Roof {
    pub x1: f32,
    pub z1: f32,
    pub x2: f32,
    pub z2: f32,
    pub thickness: f32,
}
}

// Ramp - right triangular prism defined by low and high opposite corners
// Convention:
// - (x1, y1, z1) is on the floor at the low edge.
// - (x2, y2, z2) is on the roof at the opposite corner (high edge).
// - Footprint is the axis-aligned rectangle spanned by (x1, z1) and (x2, z2).
// - Slope runs from the low edge to the high edge across that rectangle.
message! {
#[derive(Copy)]
struct Ramp {
    pub x1: f32,
    pub y1: f32,
    pub z1: f32,
    pub x2: f32,
    pub y2: f32,
    pub z2: f32,
}
}

// Grid cell wall/ramp/roof flags used by both server and client.
message! {
#[derive(Copy, Default)]
struct GridCell {
    pub has_north_wall: bool, // Horizontal wall at top edge (z)
    pub has_south_wall: bool, // Horizontal wall at bottom edge (z+1)
    pub has_west_wall: bool,  // Vertical wall at left edge (x)
    pub has_east_wall: bool,  // Vertical wall at right edge (x+1)
    pub has_ramp: bool,       // Cell occupied by a ramp footprint
    pub has_roof: bool,       // Cell has a roof on top
    // Ramp bases disallow walls on their entry edge
    pub ramp_base_north: bool,
    pub ramp_base_south: bool,
    pub ramp_base_west: bool,
    pub ramp_base_east: bool,
    // Ramp tops disallow walls on their exit edge
    pub ramp_top_north: bool,
    pub ramp_top_south: bool,
    pub ramp_top_west: bool,
    pub ramp_top_east: bool,
}
}

// Full grid configuration sent once on connect.
message! {
#[derive(Resource)]
struct MapLayout {
    pub grid: Vec<Vec<GridCell>>, // [row][col] - indexed by grid_z, grid_x
    pub boundary_walls: Vec<Wall>,
    pub interior_walls: Vec<Wall>,
    pub lower_walls: Vec<Wall>, // Pre-computed: boundary + interior
    pub roofs: Vec<Roof>,
    pub ramps: Vec<Ramp>,
    pub roof_edge_walls: Vec<Wall>, // Collision boxes for roof edges (prevent falling off)
}
}

impl MapLayout {
    // Check if a world position (x, z) is on a roof cell
    #[must_use]
    pub fn is_position_on_roof(&self, x: f32, z: f32) -> bool {
        // Convert world coordinates to grid coordinates
        let col = ((x + FIELD_WIDTH / 2.0) / GRID_SIZE).floor() as i32;
        let row = ((z + FIELD_DEPTH / 2.0) / GRID_SIZE).floor() as i32;

        // Check bounds
        if row < 0 || row >= GRID_ROWS || col < 0 || col >= GRID_COLS {
            return false;
        }

        self.grid[row as usize][col as usize].has_roof
    }
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
message! {
#[derive(Copy)]
struct Item {
    pub item_type: ItemType,
    pub pos: Position,
}
}

// Ghost - a ghost moving around the map.
message! {
#[derive(Copy)]
struct Ghost {
    pub pos: Position,
    pub vel: Velocity,
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
    pub face_dir: f32,   // radians - yaw direction player is facing when shooting
    pub face_pitch: f32, // radians - pitch (up/down) when shooting
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
    pub grid_config: MapLayout,
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
// Server to Client: Player speed update with position for reconciliation.
struct SSpeed {
    pub id: PlayerId,
    pub speed: Speed,
    pub pos: Position,
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
    pub face_dir: f32,   // radians - yaw direction player is facing when shooting
    pub face_pitch: f32, // radians - pitch (up/down) when shooting
}
}

message! {
// Server to Client: Periodic game state update for all players.
struct SUpdate {
    pub seq: u32,
    pub players: Vec<(PlayerId, Player)>,
    pub items: Vec<(ItemId, Item)>,
    pub ghosts: Vec<(GhostId, Ghost)>,
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
// Server to Client: Player status effects changed.
struct SPlayerStatus {
    pub id: PlayerId,
    pub speed_power_up: bool,
    pub multi_shot_power_up: bool,
    pub reflect_power_up: bool,
    pub phasing_power_up: bool,
    pub ghost_hunt_power_up: bool,
    pub stunned: bool,
}
}

message! {
// Server to Client: Echo response.
struct SEcho {
    pub timestamp_nanos: u64,
}
}

message! {
// Server to Client: Ghost direction changed.
struct SGhost {
    pub id: GhostId,
    pub ghost: Ghost,
}
}

message! {
// Server to Client: Player collected a cookie.
struct SCookieCollected {}
}

message! {
// Server to Client: Ghost hit a player.
struct SGhostHit {}
}

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
