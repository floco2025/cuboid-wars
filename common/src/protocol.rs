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

// Client to Server: Local player's character movement intent update.
#[derive(Debug, Clone, Encode, Decode)]
pub struct CPlayerMoveIntent {
    pub move_intent: PlayerMoveIntent,
}

// Client to Server: One-shot jump request.
#[derive(Debug, Clone, Encode, Decode)]
pub struct CJump {}

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

// Server to Client: Player movement state update for reconciliation after intent changes.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SPlayerMoveIntent {
    pub id: PlayerId,
    pub movement: PlayerMovementState,
}

// Server to Client: Server-controlled actor movement state update after intent changes.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SActorMoveIntent {
    pub id: ActorId,
    pub movement: ActorMovementState,
}

// Server to Client: Authoritative player teleport. This is not reconciliation;
// the client should apply the movement state immediately.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SPlayerTeleport {
    pub id: PlayerId,
    pub movement: PlayerMovementState,
}

// Server to Client: Authoritative actor teleport. This is not reconciliation;
// the client should apply the movement state immediately.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SActorTeleport {
    pub id: ActorId,
    pub movement: ActorMovementState,
}

// Server to Client: Actor was destroyed at this position before respawning.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SActorDestroyed {
    pub id: ActorId,
    pub pos: Position,
}

// Server to Client: Player started a jump with authoritative movement state.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SJump {
    pub id: PlayerId,
    pub movement: PlayerMovementState,
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
    pub actors: Vec<(ActorId, Actor)>,
    pub items: Vec<(ItemId, Item)>,
}

// Server to Client: Player was hit by a projectile.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SHit {
    pub id: PlayerId,   // Player who was hit
    pub hit_dir_x: f32, // Direction of hit (normalized)
    pub hit_dir_z: f32, // Direction of hit (normalized)
}

// Server to Client: Actor was hit by a projectile.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SActorHit {
    pub id: ActorId,
}

// Server to Client: Player status effects changed.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct SPlayerStatus {
    pub id: PlayerId,
    pub speed_power_up: bool,
    pub multi_shot_power_up: bool,
    pub phasing_power_up: bool,
    pub anti_gravity_power_up: bool,
    pub stunned: bool,
    // Held key inventory. Kept sorted ascending on the server so the encoded
    // bytes are deterministic and the client can change-detect via a single
    // equality test.
    pub held_keys: Vec<BarrierKindId>,
}

// Server to Client: Echo response.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SEcho {
    pub timestamp_nanos: u64,
}

// Server to Client: Player collected a cookie.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SCookieCollected {}

// ============================================================================
// Message Envelopes
// ============================================================================

// All client to server messages
#[derive(Debug, Clone, Encode, Decode)]
pub enum ClientMessage {
    Login(CLogin),
    Logoff(CLogoff),
    PlayerMoveIntent(CPlayerMoveIntent),
    Jump(CJump),
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
    PlayerMoveIntent(SPlayerMoveIntent),
    ActorMoveIntent(SActorMoveIntent),
    PlayerTeleport(SPlayerTeleport),
    ActorTeleport(SActorTeleport),
    ActorDestroyed(SActorDestroyed),
    Jump(SJump),
    Face(SFace),
    Shot(SShot),
    Update(SUpdate),
    Hit(SHit),
    ActorHit(SActorHit),
    PlayerStatus(SPlayerStatus),
    Echo(SEcho),
    CookieCollected(SCookieCollected),
}
