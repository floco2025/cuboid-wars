// Wire protocol between client and server.
//
// Server→client messages fall into four roles. When adding a new message,
// pick the smallest role that fits — most shared "X changed" things belong
// in the snapshot, not a new event.
//
// 1. Bootstrap (`SInit`) — sent once at connect with session-level state
//    (`PlayerId`, static `MapLayout`, per-map `MapSettings`).
//
// 2. State snapshot (`SSnapshot`) — the authoritative current state of every
//    player, actor, and item, broadcast at `SNAPSHOT_HZ`. Sole vehicle for
//    presence: a player appears in the first `SSnapshot` they show up in and
//    disappears in the first they're absent from. Self-healing — a dropped
//    snapshot is forgiven by the next one. Presence includes pre-presence:
//    `spawning_actors` carries reserved actor spawns during their warning
//    window, so clients render a beam-in ghost before the actor exists.
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
//        cues would lag by a tick. `SActorBeam` ships the burst's start
//        moment and duration, which the 4 Hz snapshot can't carry.
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
//    job, not the protocol's. Used today for quest assignment / progress /
//    completion (`SQuestsAssigned`, `SQuestProgress`, `SQuestCompleted`).
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
    pub map_settings: MapSettings,
    // Per-actor-kind blast radius (m) from the server's combat config, so
    // explosion VFX can telegraph the true danger area. Sorted by kind for
    // deterministic encoding.
    pub actor_explosion_radii: Vec<(String, f32)>,
    // Blast radius (m) of a dying player's explosion, same purpose.
    pub player_explosion_radius: f32,
}

// --- Snapshot ---

// Periodic full-world snapshot. Sole source of truth for player/actor/item
// presence; one-shot cues are paired against it for sub-tick latency.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SSnapshot {
    pub seq: u32,
    pub players: Vec<(PlayerId, Player)>,
    pub actors: Vec<(ActorId, Actor)>,
    // Reserved spawns still in their warning window. An id moves from here
    // to `actors` in the snapshot where the actor materializes.
    pub spawning_actors: Vec<(ActorId, SpawningActor)>,
    pub items: Vec<(ItemId, Item)>,
    // Barrier kinds currently fully open (pressure-plate threshold met).
    // Empty in v1 maps with no plates. Client hides matching barriers; server
    // unions this with each player's `held_keys` for the collision filter.
    pub open_barrier_kinds: Vec<BarrierKindId>,
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

// A player died. Drives the immediate client-side death-state transition
// (overlay + freeze for the dying player, entity teardown for others).
// `SSnapshot`'s next snapshot is the fallback.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SPlayerDeath {
    pub id: PlayerId,
    // Server-authoritative position at the moment of death. The snapshot can't
    // carry it — the victim is already gone from the next snapshot — so the cue
    // does. The client snaps the dying entity here: local prediction may have
    // drifted from the server before reconciliation converged, and the corpse
    // stays visible (top-down death view) on the true death spot.
    pub pos: Position,
    // Player who landed the killing blow; `None` for non-player causes
    // (fall, actor explosion, future environmental). Drives the
    // client-side message feed's "A → B" entry vs "A died" entry.
    pub killer: Option<PlayerId>,
    // The victim's post-death score (death penalty already applied) so the
    // HUD updates on the death tick rather than waiting for the next
    // snapshot. Snapshot is still authoritative.
    pub victim_score: i32,
    // The killer's post-kill score (kill bonus already applied), if there
    // is one. `None` for non-player causes.
    pub killer_score: Option<i32>,
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
    // The killer's post-kill score (kill bonus already applied) so the HUD
    // bumps on the kill tick rather than waiting for the next snapshot.
    // `None` when killer is `None`.
    pub killer_score: Option<i32>,
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

// Player took damage from a hard landing. Unicast to the victim. Pairs with
// `SPlayerHit` but for falls — same role (post-damage health for HUD +
// directional camera wiggle, but on the vertical axis). Lethal falls also
// surface `SPlayerDeath` on the same tick.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SPlayerFallDamage {
    pub id: PlayerId,
    pub health: Health,
}

// Blast result for a surviving victim. Unicast to the victim: health updates
// the HUD on the damage tick, the absolute velocities keep prediction aligned,
// and direction/strength drive immediate local feedback.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SPlayerBlast {
    pub id: PlayerId,
    pub health: Health,
    pub vertical_velocity: f32,
    pub velocity_x: f32,
    pub velocity_z: f32,
    pub direction_x: f32,
    pub direction_z: f32,
    pub strength: f32,
}

// Actor was hit by a projectile. Drives the `hit_actor` sound on the
// shooter's client and carries the post-hit health so floating health
// bars update on the impact tick instead of waiting for the next
// snapshot. Snapshot remains the system of record; this is just a
// latency cut.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SActorHit {
    pub id: ActorId,
    pub health: Health,
}

// Actor locked a laser burst onto a player. The beam must appear on its
// start frame (sub-tick latency) and the start is edge-triggered — the
// client renders it for `duration_secs`, anchored each frame to its own
// interpolated actor and target entities, so the tracking beam needs no
// follow-up updates and no end cue (it despawns on expiry or when either
// endpoint entity disappears). Durable damage state still rides
// `SPlayerHit` and the snapshot; a missed cue costs only the visual.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SActorBeam {
    pub id: ActorId,
    pub target: PlayerId,
    pub duration_secs: f32,
}

// Player status flags changed (power-up gained/lost, stun toggle). The same
// flags are also in `SSnapshot`, but this event is the edge trigger that fires
// the associated sounds exactly once at the transition.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct SPlayerStatus {
    pub id: PlayerId,
    // One bool per `PowerUpKind`, indexed by `PowerUpKind::index()`.
    pub power_ups: [bool; PowerUpKind::COUNT],
    pub stunned: bool,
    // Held key inventory. Kept sorted ascending on the server so the encoded
    // bytes are deterministic and the client can change-detect via a single
    // equality test.
    pub held_keys: Vec<BarrierKindId>,
}

impl SPlayerStatus {
    #[must_use]
    pub const fn power_up(&self, kind: PowerUpKind) -> bool {
        self.power_ups[kind.index()]
    }
}

// Player collected a cookie. Sent only to the collecting player; drives the
// pickup sound AND carries the post-pickup score for snappier HUD reaction.
// The snapshot remains the system of record — this is just an early-arriving
// redundant copy; the next `SSnapshot` will agree.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SCookieCollected {
    pub score: i32,
}

// Player collected a health potion. Unicast one-shot for the pickup sound +
// the post-pickup health value, so the HUD updates immediately rather than
// waiting up to a snapshot interval. Exists because `SPlayerStatus` only
// carries durable booleans and the potion has none to flip; the snapshot's
// `Player.health` is still the system of record.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SHealthPotionCollected {
    pub health: Health,
}

// A pressure plate transitioned from "unpressed" to "pressed" this tick
// (some alive player just stepped onto its inner-25% rect). Broadcast —
// any client may hear the click. Edge-triggered side-effect; durable
// state (which kinds are currently open) rides `SSnapshot`.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SPressurePlatePressed {}

// Mirror of `SPressurePlatePressed`: a plate transitioned from "pressed"
// to "unpressed" this tick (last alive player stepped off). Broadcast.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SPressurePlateReleased {}

// --- Per-client state events (private, durable) ---

// One quest in an `SQuestsAssigned` batch. Carries display strings inline so
// the client never needs a separate quest catalog: `title` is the short panel
// label, `description` the longer announcement body. `threshold` is the
// progress denominator; `progress` is 0 for a fresh login grant but may be
// non-zero if a quest is (re)assigned mid-session against existing progress.
#[derive(Debug, Clone, Encode, Decode)]
pub struct NewQuest {
    pub id: QuestId,
    pub title: String,
    pub description: String,
    pub progress: u32,
    pub threshold: u32,
    // Display rank: the quest's position in the server's quest catalog. The
    // client sorts the panel and respawn announcement by it so authoring order
    // in `gameplay.json` drives display order everywhere.
    pub order: u32,
}

// One or more quests assigned to a specific player. Unicast. Batched so quests
// granted together — at login or from a future in-game quest-giver — surface
// in a single combined announcement banner. Installs lasting client state (the
// quest panel reads it); the announcement banner is presentation-only and its
// duration lives in `client.json`'s `hud.banner`.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SQuestsAssigned {
    pub quests: Vec<NewQuest>,
}

// A quest's progress advanced (without completing). Unicast. Carries the
// absolute progress value, not a delta, so a client can ignore a reordered or
// stale update by keeping the max — separate uni-streams don't guarantee order.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SQuestProgress {
    pub id: QuestId,
    pub progress: u32,
}

// Quest just completed by a specific player. Unicast; marks the quest done in
// the client's panel and fires the completion banner.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SQuestCompleted {
    pub id: QuestId,
    pub completed_text: String,
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
    PlayerFallDamage(SPlayerFallDamage),
    PlayerBlast(SPlayerBlast),
    ActorHit(SActorHit),
    ActorBeam(SActorBeam),
    PlayerStatus(SPlayerStatus),
    CookieCollected(SCookieCollected),
    HealthPotionCollected(SHealthPotionCollected),
    PressurePlatePressed(SPressurePlatePressed),
    PressurePlateReleased(SPressurePlateReleased),
    // Per-client state events
    QuestsAssigned(SQuestsAssigned),
    QuestProgress(SQuestProgress),
    QuestCompleted(SQuestCompleted),
    // Diagnostic
    Pong(SPong),
}
