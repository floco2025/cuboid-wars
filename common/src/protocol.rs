// Wire protocol between client and server.
//
// Server→client messages fall into three roles. When adding a new message,
// pick the smallest role that fits — most "X changed" things belong in the
// snapshot, not a new event.
//
// 1. Bootstrap (`SInit`) — sent once at connect with session-level state
//    (`PlayerId`, static `MapLayout`).
//
// 2. State snapshot (`SUpdate`) — the authoritative current state of every
//    player, actor, and item, broadcast every server tick. Sole vehicle for
//    presence: a player appears the tick they first show up in `SUpdate` and
//    disappears the tick they're absent. Self-healing — a dropped tick is
//    forgiven by the next one.
//
// 3. One-shot cues — short messages that fire at the moment of a discrete
//    state change. They exist only when the snapshot can't carry the cue,
//    which is one of:
//      * Sub-tick latency matters. Movement prediction needs intent changes
//        (`SPlayerMoveIntent`, `SJump`, `SFace`, `SShot`) faster than tick
//        cadence; camera shake from `SPlayerHit` needs to land on the impact
//        frame, not 1–2 ticks later.
//      * Edge-triggered, not level-triggered. "You just picked up a power-up"
//        is a transition with an associated sound (`SPlayerStatus`,
//        `SCookieCollected`). The snapshot also carries `speed_power_up:
//        true`, but a level-triggered handler would play the sound every
//        tick the flag was set. The event fires the sound exactly once at
//        the transition; the snapshot keeps the HUD icon correct if the
//        event was dropped.
//      * The cue carries information the snapshot doesn't. `SPlayerHit`
//        ships hit direction for directional camera shake; `SActorDestroyed`
//        triggers the explosion VFX (without it, the actor would silently
//        vanish from the snapshot).
//
// `CPing` / `SPong` are a separate diagnostic channel for RTT measurement.

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

// Client to Server: Ping request with timestamp (Duration since app start, serialized as nanoseconds).
// Echoed back by the server as `SPong` so the client can measure RTT.
#[derive(Debug, Clone, Encode, Decode)]
pub struct CPing {
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
pub struct SPlayerHit {
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

// Server to Client: Pong response — server echoes the `CPing` timestamp back
// unchanged so the client can compute RTT from the round trip.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SPong {
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
    Ping(CPing),
}

// All server to client messages
#[derive(Debug, Clone, Message, Encode, Decode)]
pub enum ServerMessage {
    Init(SInit),
    PlayerMoveIntent(SPlayerMoveIntent),
    ActorMoveIntent(SActorMoveIntent),
    ActorDestroyed(SActorDestroyed),
    Jump(SJump),
    Face(SFace),
    Shot(SShot),
    Update(SUpdate),
    PlayerHit(SPlayerHit),
    ActorHit(SActorHit),
    PlayerStatus(SPlayerStatus),
    Pong(SPong),
    CookieCollected(SCookieCollected),
}
