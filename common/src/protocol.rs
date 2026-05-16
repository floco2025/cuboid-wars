// Wire protocol between client and server.
//
// Server→client messages fall into four roles. When adding a new message,
// pick the smallest role that fits — most shared "X changed" things belong
// in the snapshot, not a new event.
//
// 1. Bootstrap (`SInit`) — sent once at connect with session-level state
//    (`PlayerId`, static `MapLayout`).
//
// 2. State snapshot (`SSnapshot`) — the authoritative current state of every
//    player, actor, and item, broadcast at `SNAPSHOT_HZ`. Sole vehicle for
//    presence: a player appears in the first `SSnapshot` they show up in and
//    disappears in the first they're absent from. Self-healing — a dropped
//    snapshot is forgiven by the next one.
//
//    Projectiles are the deliberate exception. They are short-lived, fast,
//    and numerous, so they are replicated as shot intents (`SShot`) rather
//    than snapshot entities. Clients simulate them only for presentation;
//    authoritative hit/death outcomes still come from the server.
//
// 3. One-shot cues — short messages that fire at the moment of a discrete
//    state change in the *shared* world. They sit alongside the snapshot,
//    not replacing it, and exist only when the snapshot alone can't carry
//    the cue, which is one of:
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
//        ships hit direction for directional camera shake; `SActorDeath` /
//        `SPlayerDeath` trigger immediate death-side work (VFX, overlay,
//        entity teardown) one tick before the snapshot would catch up.
//        Without them, the actor or player would silently disappear and the
//        cues would lag by a tick.
//    One-shot cues are *ephemeral*: a missed cue at most costs the
//    associated side effect (a sound, a shake); the snapshot reconciles the
//    durable state.
//
// 4. Per-client state events — durable per-player state that has no place in
//    the world snapshot because other clients don't need it. Unicast to the
//    affected player only. Unlike one-shot cues these install lasting client
//    state (e.g. an active quest's announcement text); the client treats
//    receipt as authoritative until a follow-up message updates it. There
//    is no snapshot-side fallback — recovery from packet loss is QUIC's
//    job, not the protocol's. Used today for: quest assignment / completion
//    (`SQuestNew`, `SQuestAchieved`).
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
//
// Ordered by role: bootstrap → snapshot → real-time intent → one-shot cues
// → diagnostic. Matches the protocol-model doc comment at the top of this
// file.

// --- Bootstrap ---

// Initial connection acknowledgment with assigned player ID + map layout.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SInit {
    pub id: PlayerId,
    pub map_layout: MapLayout,
}

// --- Snapshot ---

// Periodic full-world snapshot. Sole source of truth for player/actor/item
// presence; one-shot cues are paired against it for sub-tick latency.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SSnapshot {
    pub seq: u32,
    pub players: Vec<(PlayerId, Player)>,
    pub actors: Vec<(ActorId, Actor)>,
    pub items: Vec<(ItemId, Item)>,
}

// --- Real-time intent (sub-tick latency for prediction) ---

// Player movement intent change for client-side prediction of remote players.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SPlayerMoveIntent {
    pub id: PlayerId,
    pub movement: PlayerMovementState,
}

// Actor movement intent change.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SActorMoveIntent {
    pub id: ActorId,
    pub movement: ActorMovementState,
}

// Player started a jump with authoritative vertical velocity.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SJump {
    pub id: PlayerId,
    pub movement: PlayerMovementState,
}

// Player facing direction update (yaw, radians).
#[derive(Debug, Clone, Encode, Decode)]
pub struct SFace {
    pub id: PlayerId,
    pub dir: f32,
}

// Player fired a shot. Projectile entities are intentionally not carried in
// `SSnapshot`: clients spawn and simulate them for presentation, while the
// server runs its own projectile simulation for authoritative hit logic.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SShot {
    pub id: PlayerId,
    pub face_dir: f32,
    pub face_pitch: f32,
}

// --- One-shot cues (edge-triggered FX/state changes) ---

// Player died at this position. Drives the immediate client-side death-state
// transition (overlay + freeze for the dying player, entity teardown for
// others). `SSnapshot`'s next snapshot is the fallback.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SPlayerDeath {
    pub id: PlayerId,
    pub pos: Position,
    // Player who landed the killing blow; `None` for non-player causes
    // (fall, actor explosion, future environmental). Drives the
    // client-side message feed's "A → B" entry vs "A died" entry.
    pub killer: Option<PlayerId>,
}

// Actor died at this position. Triggers the explosion VFX + sound and the
// local entity teardown. `SSnapshot`'s next snapshot is the fallback.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SActorDeath {
    pub id: ActorId,
    pub pos: Position,
    // Player who landed the killing blow; `None` if the actor died from
    // chain-explosion damage or other non-player causes.
    pub killer: Option<PlayerId>,
}

// Player was hit by a projectile. Carries hit direction so the victim's
// camera shake reads directionally; snapshot can't carry this. Also
// carries the post-damage health so the HUD health bar updates on the
// impact frame instead of waiting for the next snapshot.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SPlayerHit {
    pub id: PlayerId,
    pub hit_dir_x: f32,
    pub hit_dir_z: f32,
    pub health: Health,
}

// Actor was hit by a projectile (drives the `hit_actor` sound on the
// shooter's client).
#[derive(Debug, Clone, Encode, Decode)]
pub struct SActorHit {
    pub id: ActorId,
}

// Player status flags changed (power-up gained/lost, stun toggle). The same
// flags are also in `SSnapshot`, but this event is the edge trigger that fires
// the associated sounds exactly once at the transition.
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

// Player collected a cookie. Sent only to the collecting player; drives the
// pickup sound. Unicast because no other client needs it, but still a
// one-shot cue: the side effect is an ephemeral sound, not durable state.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SCookieCollected {}

// Player took damage from a hard landing. Unicast to the victim. Pairs with
// `SPlayerHit` but for falls — same role (post-damage health for HUD +
// directional camera wiggle, but on the vertical axis). Lethal falls also
// surface `SPlayerDeath` on the same tick.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SFallDamage {
    pub id: PlayerId,
    pub health: Health,
}

// --- Per-client state events (private, durable) ---

// New quest assigned to a specific player. Unicast; carries the
// announcement text inline so the client never needs a separate quest
// catalog. Installs lasting client state — the announcement text is cached
// and re-shown on each respawn until `SQuestAchieved` retires it. Display
// duration is presentation-only and lives in `client.json`'s
// `hud.quest_overlay`.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SQuestNew {
    pub id: QuestId,
    pub announcement_text: String,
}

// Quest just completed by a specific player. Unicast; retires the
// corresponding `SQuestNew` entry on the client and fires the achieved
// overlay.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SQuestAchieved {
    pub id: QuestId,
    pub achieved_text: String,
}

// --- Diagnostic ---

// Pong response — server echoes the `CPing` timestamp back unchanged so the
// client can compute RTT from the round trip.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SPong {
    pub timestamp_nanos: u64,
}

// ============================================================================
// Message Envelopes
// ============================================================================

// All client to server messages
#[derive(Debug, Clone, Encode, Decode)]
pub enum ClientMessage {
    Login(CLogin),
    PlayerMoveIntent(CPlayerMoveIntent),
    Jump(CJump),
    Face(CFace),
    Shot(CShot),
    Ping(CPing),
}

// All server to client messages. Variants are grouped by role to match the
// struct ordering above; new messages should land in the appropriate group.
// Note: bincode encodes the discriminant by position, so reordering touches
// the wire format — fine for an in-dev workspace where server and client
// always build from the same source.
#[derive(Debug, Clone, Message, Encode, Decode)]
pub enum ServerMessage {
    // Bootstrap
    Init(SInit),
    // Snapshot
    Snapshot(SSnapshot),
    // Real-time intent
    PlayerMoveIntent(SPlayerMoveIntent),
    ActorMoveIntent(SActorMoveIntent),
    Jump(SJump),
    Face(SFace),
    Shot(SShot),
    // One-shot cues
    PlayerDeath(SPlayerDeath),
    ActorDeath(SActorDeath),
    PlayerHit(SPlayerHit),
    ActorHit(SActorHit),
    PlayerStatus(SPlayerStatus),
    CookieCollected(SCookieCollected),
    FallDamage(SFallDamage),
    // Per-client state events
    QuestNew(SQuestNew),
    QuestAchieved(SQuestAchieved),
    // Diagnostic
    Pong(SPong),
}
