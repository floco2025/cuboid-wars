use bevy_ecs::prelude::*;
use bincode::{Decode, Encode};

pub use crate::types::*;

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
    pub map_layout: MapLayout,
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
    pub sentries: Vec<(SentryId, Sentry)>,
}

// Server to Client: Player was hit by a projectile.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SHit {
    pub id: PlayerId,   // Player who was hit
    pub hit_dir_x: f32, // Direction of hit (normalized)
    pub hit_dir_z: f32, // Direction of hit (normalized)
}

// Server to Client: Player status effects changed.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct SPlayerStatus {
    pub id: PlayerId,
    pub speed_power_up: bool,
    pub multi_shot_power_up: bool,
    pub phasing_power_up: bool,
    pub sentry_hunt_power_up: bool,
    pub stunned: bool,
}

// Server to Client: Echo response.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SEcho {
    pub timestamp_nanos: u64,
}

// Server to Client: Sentry direction changed.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SSentry {
    pub id: SentryId,
    pub sentry: Sentry,
}

// Server to Client: Player collected a cookie.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SCookieCollected {}

// Server to Client: Sentry hit a player.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SSentryHit {}

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
    Sentry(SSentry),
    CookieCollected(SCookieCollected),
    SentryHit(SSentryHit),
}
