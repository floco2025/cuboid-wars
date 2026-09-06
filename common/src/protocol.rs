// Wire protocol between client and server.
//
// Every message has a role, and the role decides its QUIC lane. `Lane` and
// the `lane()` methods at the bottom of this file are the authoritative
// assignment; the transport in `common/src/network.rs` dispatches on them
// and knows nothing else about messages.
//
// Lanes:
// * Reliable — delivered, in order, both directions, on one long-lived
//   bidirectional stream per connection.
// * Unreliable — may be lost and may arrive out of order; every handler
//   tolerates both. The transport picks the carrier per send (a datagram
//   when the message fits one packet, its own stream otherwise) and never
//   drops anything. `SSnapshot` and `SPlayerMoves` replace state wholesale,
//   so each carries the server tick it reflects and the client ignores an
//   older one; that tick is also the clock the client keeps (`ServerTick`).
//
// Roles, in both directions:
//
// 1. Bootstrap (reliable) — `CLogin` in, `SInit` out, once per connection.
//    `SInit` is the first thing the server writes on the reliable lane, and
//    the client ignores every message until it holds `SInit`. Anything that
//    can precede it is unreliable, droppable by definition, so there is no
//    pre-init buffer and no readiness handshake. On the server, `CLogin`
//    alone makes a connection `Active`; an unreliable message that overtakes
//    it is dropped with a warning.
//
// 2. State (unreliable, latest wins) — the complete periodic picture.
//    `SSnapshot` is the authoritative current state of every player, actor,
//    and item (plus shared world state such as open barrier kinds, group
//    quest status, plate gating, and placed portals), broadcast at
//    `SNAPSHOT_HZ`. Sole vehicle for presence: a player appears in the first
//    `SSnapshot` they show up in and disappears in the first they're absent
//    from. Presence includes pre-presence: `spawning_actors` carries reserved
//    actor spawns during their warning window, so clients render a beam-in
//    ghost before the actor exists. `SPlayerMoves` is the per-tick companion:
//    every active player's `PlayerMovementState` after each tick's movement,
//    the receiver's own included. It exists because the local player needs
//    an echo of each input to measure its prediction error against; since
//    the server sends it every tick anyway, every player rides it, which is
//    what retires a separate per-change player cue and the snapshot's
//    reconciliation of players. Actors and missiles have no such message:
//    nothing about them needs an echo, and a per-tick roster of the many
//    actors on a map would crowd a datagram, so they keep the per-change
//    cue plus the snapshot.
//
//    Projectiles are the deliberate exception. They are short-lived, fast,
//    and numerous, so they are replicated as shot cues (`SProjectileShot`)
//    rather than snapshot entities. Clients simulate them only for
//    presentation; authoritative hit/death outcomes still come from the
//    server. Missiles are NOT that exception: they fly for seconds and steer
//    server-side, so they are full snapshot entities reconciled like actors.
//
// 3. Cues (unreliable) — short messages that arrive ahead of the next state
//    message and are healed by it, so a lost cue costs at most a sound, a
//    shake, or a snapshot interval of latency. A cue exists only when the
//    snapshot alone can't carry it, which is one of:
//      * Sub-tick latency matters. Movement prediction inputs (`SActorMove`,
//        `SMissileMove`, `SMissileLaunch`) must
//        arrive faster than snapshot cadence so clients can dead-reckon
//        between snapshots; camera shake from `SPlayerHit` needs to land on
//        the impact frame, not 1–2 ticks later.
//      * Edge-triggered, not level-triggered. "You just picked up a power-up"
//        is a transition with an associated sound (`SPlayerStatus`,
//        `SCookieCollected`). The snapshot also carries the flag, but a
//        level-triggered handler would play the sound every tick it was set.
//        The cue fires the sound exactly once at the transition; the snapshot
//        keeps the HUD icon correct if the cue was dropped.
//      * The cue carries information the snapshot doesn't. `SPlayerHit`
//        ships hit direction for directional camera shake; `SActorDeath` /
//        `SPlayerDeath` trigger immediate death-side work (VFX, overlay,
//        entity teardown) one tick before the snapshot would catch up.
//        `SActorBeam` ships the burst's start moment and duration, which the
//        4 Hz snapshot can't carry.
//    Inbound, `CMove` is both cue and state: sent every tick, changed or
//    not, so a lost one heals at the next. `CPing` / `SPong` measure RTT on
//    the same terms.
//
// 4. Events (reliable) — messages the snapshot cannot stand in for, so loss
//    is not an option; the client treats receipt as authoritative until a
//    follow-up message updates it. `SQuestUpdates` carries durable
//    per-player quest state (unicast; every update is the complete current
//    state, so it has no ordering dependency on an earlier quest message,
//    and group quest state also rides the snapshot). `SFeed` is one
//    server-rendered line for the message feed: final text spans with
//    semantic styles the client maps to colors; public lines target everyone
//    or everyone except one player, admin replies the issuer. `SFirework`
//    starts the client-side show from a seed. Inbound events are the
//    player's actions (`CJump`, `CProjectileShot`, `CMissileShot`,
//    `CPortalShot`), which nothing could replay if lost, plus `CAdmin` and
//    `CChat`.
//
// When adding a message, pick the smallest role that fits — most shared
// "X changed" things belong in the snapshot, not a new message — and it
// takes that role's lane.
//
// The server supplies the authenticated `PlayerId` from its transport; keeping
// that ID out of the wire payload prevents clients from choosing their own
// identity.

use bevy_ecs::prelude::Resource;
use bincode::{Decode, Encode};

use crate::config::GameplayBootstrap;
pub use crate::types::*;

// ============================================================================
// Client Messages
// ============================================================================

// Client to Server: Login request.
#[derive(Debug, Clone, Encode, Decode)]
pub struct CLogin {
    pub name: String,
}

// The local player's steady-state input: movement intent plus facing in
// radians, carried by `CMove`.
#[derive(Debug, Clone, Copy, Encode, Decode)]
pub struct PlayerInput {
    pub move_intent: PlayerMoveIntent,
    pub face_yaw: f32,
}

impl PlayerInput {
    #[must_use]
    pub fn is_finite(&self) -> bool {
        self.move_intent.is_finite() && self.face_yaw.is_finite()
    }
}

// Client to Server: the input, committed every tick whether it changed or
// not, so a lost commit heals at the next one. It replaces state wholesale,
// so it carries a sequence and the server ignores a commit older than the
// last one it took in; the sequence names the commit, which is why it is
// not the client's tick estimate. `hops` is how many portal crossings the
// client's own simulation of its player has made: the intent is expressed on
// that side of them, and the server applies it only once its player has made
// the same ones.
#[derive(Debug, Clone, Encode, Decode)]
pub struct CMove {
    pub seq: u32,
    pub input: PlayerInput,
    pub hops: u32,
}

// Client to Server: One-shot jump request.
#[derive(Debug, Clone, Encode, Decode)]
pub struct CJump {}

// Client to Server: Projectile shot fired.
#[derive(Debug, Clone, Encode, Decode)]
pub struct CProjectileShot {
    pub face_yaw: f32,   // radians - yaw direction player is facing when shooting
    pub face_pitch: f32, // radians - pitch (up/down) when shooting
    pub pattern: Option<String>,
}

// Client to Server: fire a seeking missile at the locked target. Only sent
// while the client has a lock; the server re-validates (target alive, in
// range, sight clear) before spawning.
#[derive(Debug, Clone, Encode, Decode)]
pub struct CMissileShot {
    // `None` = unguided shot along the aim; only honored when
    // `missiles.require_lock` is off.
    pub target: Option<HomingTarget>,
    pub face_yaw: f32,   // radians - yaw when firing
    pub face_pitch: f32, // radians - pitch when firing
}

// Client to Server: place one end allowed by the shooter's portal assignment. The server
// re-derives the eye ray from yaw/pitch, casts it at world geometry, and
// answers with `SPortalOpened` — or silently fizzles on a miss; the client
// spawns nothing locally.
#[derive(Debug, Clone, Encode, Decode)]
pub struct CPortalShot {
    pub end: PortalEnd,
    pub face_yaw: f32,   // radians - yaw when firing
    pub face_pitch: f32, // radians - pitch (up/down) when firing
}

// Client to Server: Ping request with timestamp (Duration since app start, serialized as nanoseconds).
// Echoed back by the server as `SPong` so the client can measure RTT.
#[derive(Debug, Clone, Encode, Decode)]
pub struct CPing {
    pub timestamp_nanos: u64,
}

// Client to Server: raw admin command string (e.g. "rain start"). The
// client stays dumb — parsing, execution, authorization, and the reply
// text (answered as an `SFeed` line) all live server-side, so new commands
// never touch the protocol.
#[derive(Debug, Clone, Encode, Decode)]
pub struct CAdmin {
    pub command: String,
}

// Client to Server: raw chat line (a slashless console entry). The server
// sanitizes it and broadcasts a rendered `SFeed` line.
#[derive(Debug, Clone, Encode, Decode)]
pub struct CChat {
    pub text: String,
}

// ============================================================================
// Server Messages
// ============================================================================
//
// Ordered by role: bootstrap → state → cues → events. Matches the
// protocol-model doc comment at the top of this file.

// --- Bootstrap ---

// Initial connection acknowledgment with per-player and shared world state.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SInit {
    pub player: PlayerBootstrap,
    pub world: WorldBootstrap,
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct PlayerBootstrap {
    pub id: PlayerId,
    pub portal_access: PortalAccess,
}

#[derive(Debug, Clone, Encode, Decode, Resource)]
pub struct WorldBootstrap {
    pub gameplay: GameplayBootstrap,
    pub map: MapBootstrap,
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct MapBootstrap {
    pub layout: MapLayout,
    pub settings: MapSettings,
    // Derived from the map's placed keys; the client creates only the HUD key
    // slots available on this map.
    pub key_kinds: Vec<BarrierKindId>,
}

// --- State ---

// A blend between two named client-side lighting presets ("bright", "dim",
// "dark"): the rendered look is `from` faded toward `to` by `blend`. A
// plain preset is the degenerate blend (`from == to`, blend 0).
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct LightingBlend {
    pub from: String,
    pub to: String,
    pub blend: f32,
}

// Periodic full-world snapshot. Sole source of truth for player/actor/item
// presence; cues are paired against it for sub-tick latency.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SSnapshot {
    // The server tick whose state this is; the client ignores a snapshot
    // older than the last one it applied.
    pub tick: u32,
    pub players: Vec<(PlayerId, Player)>,
    pub actors: Vec<(ActorId, Actor)>,
    // Reserved spawns still in their warning window. An id moves from here
    // to `actors` in the snapshot where the actor materializes.
    pub spawning_actors: Vec<(ActorId, SpawningActor)>,
    pub items: Vec<(ItemId, Item)>,
    // In-flight missiles. Unlike projectiles, missiles ARE snapshot entities:
    // they fly for seconds and steer server-side, so presence and position
    // self-heal here while `SMissileMove` carries course changes.
    pub missiles: Vec<(MissileId, Missile)>,
    // What the pressure plates hold right now: open barrier kinds (the
    // client hides them; the server unions them with each player's
    // `held_keys` for the collision filter) and powered bridge kinds (solid
    // and lit on both sides). Empty on maps with no plates.
    pub plates: PlateState,
    // Every unlocked `shared` / `everyone` quest. Completed ones stay listed
    // (completions are latched for the session) so late joiners and dropped
    // cues self-heal.
    pub quests: Vec<QuestGroupStatus>,
    // Plate purposes still locked behind a quest: the plates that solve a
    // quest are inert and hidden until that quest unlocks. Sorted, usually
    // empty.
    pub locked_plate_purposes: Vec<PlatePurpose>,
    // Server-scheduled weather, 0.0 (clear) to 1.0 (full rain). Durable
    // level-triggered state, so it rides the snapshot — late joiners enter
    // mid-storm correctly and a dropped packet self-heals. Clients smooth
    // the 4 Hz steps and drive all rain presentation from it.
    pub rain_intensity: f32,
    // Server-driven lighting: which two presets the world is between and
    // how far. Same snapshot rationale as the rain intensity. The client
    // resolves the names against its configured looks and eases toward the
    // blended result.
    pub lighting: LightingBlend,
    // Every placed portal end, sorted by pair and
    // end. Durable, everyone-visible world objects: the snapshot is their
    // system of record, and late joiners and dropped `SPortalOpened` cues
    // self-heal here.
    pub portals: Vec<Portal>,
}

// Every active player's movement state after this tick's movement, sent to
// every player once per tick, the receiver's own included. Remote players
// take their intent, facing, and vertical velocity from it; every player
// reconciles against it. Presence is the snapshot's alone: a client ignores
// entries for players it does not know. Like `SSnapshot`, it carries the
// server tick it reflects and the client ignores an older one; paired with
// each entry's `move_seq`, that tick is how the client corrects its clock.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SPlayerMoves {
    pub tick: u32,
    pub moves: Vec<PlayerMove>,
}

#[derive(Debug, Clone, Copy, Encode, Decode)]
pub struct PlayerMove {
    pub id: PlayerId,
    pub movement: PlayerMovementState,
    // The newest `CMove.seq` the server had taken in before this state was
    // computed; a commit held for a portal crossing counts, its input
    // unapplied. Only the player it belongs to can use it: they compare
    // `movement.pos` with the position they predicted after that same
    // `CMove`, a measured error rather than an extrapolated one.
    pub move_seq: u32,
    // How many portal crossings the server's player has made. A client
    // steers or reconciles a player only from a state whose count matches
    // its own simulation of that player (`PlayerInfo::hops` on the client).
    pub hops: u32,
}

// --- Cues (ahead of the next snapshot, healed by it) ---

// Player fired a shot. Projectile entities are intentionally not carried in
// `SSnapshot`: clients spawn and simulate them for presentation, while the
// server runs its own projectile simulation for authoritative hit logic.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SProjectileShot {
    pub id: PlayerId,
    pub face_yaw: f32,
    pub face_pitch: f32,
    pub pattern: Option<String>,
}

// Actor movement change.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SActorMove {
    pub id: ActorId,
    pub movement: ActorMovementState,
}

// A missile launched. Broadcast to all (including the shooter — clients do
// not predict missile spawns; the server owns the whole flight). The next
// snapshot is the presence fallback.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SMissileLaunch {
    pub id: MissileId,
    pub shooter: PlayerId,
    pub movement: MissileMovementState,
}

// Missile course change. Broadcast when the server-steered direction drifts
// past an epsilon from the last broadcast; clients dead-reckon a straight
// line in between and reconcile against the carried position.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SMissileMove {
    pub id: MissileId,
    pub movement: MissileMovementState,
}

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
    // Player credited with the kill; `None` for non-player causes and
    // self-kills. Only pairs with `killer_score` below — the feed line
    // (with the full cause) is a separate `SFeed`.
    pub killer: Option<PlayerId>,
    // The victim's post-death score (death penalty already applied) so the
    // HUD updates on the death tick rather than waiting for the next
    // snapshot. Snapshot is still authoritative.
    pub victim_score: i32,
    // The killer's post-kill score (kill bonus already applied), if there
    // is one. `None` for non-player causes.
    pub killer_score: Option<i32>,
    // Whether the death detonates. A fall out of the world does not: the
    // blast would reach nothing that deep, and there is nothing to show.
    pub explodes: bool,
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

// Missile detonated at this position (impact, lifetime, or stall). Triggers
// the explosion VFX + sound and the local teardown; disappearance from the
// next snapshot is the fallback.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SMissileDetonated {
    pub id: MissileId,
    pub pos: Position,
}

// What damaged the player in an `SPlayerHit` — clients tune the camera
// shake per source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum HitKind {
    Projectile,
    Beam,
}

// Player was hit by a projectile or a laser beam. Carries hit direction so
// the victim's camera shake reads directionally; snapshot can't carry this.
// Also carries the post-damage health so the HUD health bar updates on the
// impact frame instead of waiting for the next snapshot.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SPlayerHit {
    pub id: PlayerId,
    pub kind: HitKind,
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
// the HUD on the damage tick and the absolute velocities keep prediction
// aligned. Direction/strength ride along for future feedback use — the
// client currently plays none (the knockback itself is the feedback).
#[derive(Debug, Clone, Encode, Decode)]
pub struct SPlayerBlast {
    pub id: PlayerId,
    pub health: Health,
    pub vertical_velocity: f32,
    pub velocity_x: f32,
    pub velocity_z: f32,
    pub hit_dir_x: f32,
    pub hit_dir_z: f32,
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

// Player collected a health potion. Unicast cue for the pickup sound +
// the post-pickup health value, so the HUD updates immediately rather than
// waiting up to a snapshot interval. Exists because `SPlayerStatus` only
// carries durable booleans and the potion has none to flip; the snapshot's
// `Player.health` is still the system of record.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SHealthPotionCollected {
    pub health: Health,
}

// Player collected a missile pack (or was granted missiles). Unicast to the
// collector: pickup sound + immediate HUD count. Modeled on
// `SHealthPotionCollected`; the snapshot's `Player.missiles` is the system
// of record.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SMissilesCollected {
    pub missiles: u32,
}

// A pressure plate transitioned this tick: `pressed` is true when some alive
// player just stepped onto its inner-25% rect, false when the last alive
// player stepped off. Broadcast — any client may hear the click. Edge-triggered
// side-effect; durable state (which kinds are currently open) rides `SSnapshot`.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SPressurePlate {
    pub pressed: bool,
}

// A portal end was placed or moved. Latency cue for the placement visual and
// portal-gun sound, plus keeping every client's portal geometry fresh: portal
// crossings are not messaged at all, each client simulates every player's
// crossings from the shared geometry, so a placement must reach observers
// quickly. The snapshot's `portals` list is the system of record.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SPortalOpened {
    pub shooter: PlayerId,
    pub portal: Portal,
}

// Pong response — server echoes the `CPing` timestamp back unchanged so the
// client can compute RTT from the round trip.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SPong {
    pub timestamp_nanos: u64,
}

// --- Events (delivered; nothing in the snapshot could stand in) ---

// One server-rendered message-feed line. Spans carry semantic styles so the
// client only maps them to its configured presentation.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct SFeed {
    pub spans: Vec<FeedSpan>,
}

// Complete client-visible state for one assigned quest. Static display data is
// deliberately repeated on updates: quest traffic is sparse, and making each
// update independently applicable is simpler than imposing cross-stream order.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct QuestState {
    pub id: QuestId,
    pub title: String,
    pub description: String,
    pub completed_text: String,
    pub threshold: u32,
    pub scope: QuestScope,
    // Authored position in the selected map's quest list.
    pub order: u32,
    pub status: QuestStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum QuestUpdateReason {
    Assigned,
    Progressed,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct QuestUpdate {
    pub reason: QuestUpdateReason,
    pub quest: QuestState,
}

// Batched for initial assignment and future multi-quest unlocks; ordinary
// progress and completion updates usually contain one entry. Unicast for an
// individual quest and sent independently to each active player for group
// quests because `QuestState` includes that player's own progress.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SQuestUpdates {
    pub updates: Vec<QuestUpdate>,
}

// Admin `/firework` or the firework plates: play the client-side firework
// show. Pure presentation — the server broadcasts the seed and forgets; every
// client derives the same choreography from it, so all clients see the same
// show.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SFirework {
    pub seed: u64,
}

// ============================================================================
// Message Envelopes
// ============================================================================

// All client to server messages
#[derive(Debug, Clone, Encode, Decode)]
pub enum ClientMessage {
    // Bootstrap
    Login(CLogin),
    // Cues
    Move(CMove),
    Ping(CPing),
    // Events
    Jump(CJump),
    ProjectileShot(CProjectileShot),
    MissileShot(CMissileShot),
    PortalShot(CPortalShot),
    Admin(CAdmin),
    Chat(CChat),
}

// All server to client messages. Variants are grouped by role to match the
// struct ordering above; new messages should land in the appropriate group.
// Note: bincode encodes the discriminant by position, so reordering touches
// the wire format — fine for an in-dev workspace where server and client
// always build from the same source.
#[expect(
    clippy::large_enum_variant,
    reason = "SInit intentionally carries the complete bootstrap state"
)]
#[derive(Debug, Clone, Encode, Decode)]
pub enum ServerMessage {
    // Bootstrap
    Init(SInit),
    // State
    Snapshot(SSnapshot),
    PlayerMoves(SPlayerMoves),
    // Cues
    ProjectileShot(SProjectileShot),
    ActorMove(SActorMove),
    MissileLaunch(SMissileLaunch),
    MissileMove(SMissileMove),
    PlayerDeath(SPlayerDeath),
    ActorDeath(SActorDeath),
    MissileDetonated(SMissileDetonated),
    PlayerHit(SPlayerHit),
    PlayerFallDamage(SPlayerFallDamage),
    PlayerBlast(SPlayerBlast),
    ActorHit(SActorHit),
    ActorBeam(SActorBeam),
    PlayerStatus(SPlayerStatus),
    CookieCollected(SCookieCollected),
    HealthPotionCollected(SHealthPotionCollected),
    MissilesCollected(SMissilesCollected),
    PressurePlate(SPressurePlate),
    PortalOpened(SPortalOpened),
    Pong(SPong),
    // Events
    Feed(SFeed),
    QuestUpdates(SQuestUpdates),
    Firework(SFirework),
}

// Wire sequence numbers wrap; `seq` is newer than `last` when it is ahead by
// less than half the range.
#[must_use]
pub const fn sequence_is_newer(seq: u32, last: u32) -> bool {
    seq != last && seq.wrapping_sub(last) < (1 << 31)
}

// The QUIC lane a message rides; see the top-of-file comment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lane {
    Reliable,
    Unreliable,
}

impl ClientMessage {
    #[must_use]
    pub const fn lane(&self) -> Lane {
        match self {
            Self::Login(_)
            | Self::Jump(_)
            | Self::ProjectileShot(_)
            | Self::MissileShot(_)
            | Self::PortalShot(_)
            | Self::Admin(_)
            | Self::Chat(_) => Lane::Reliable,
            Self::Move(_) | Self::Ping(_) => Lane::Unreliable,
        }
    }
}

impl ServerMessage {
    #[must_use]
    pub const fn lane(&self) -> Lane {
        match self {
            Self::Init(_) | Self::Feed(_) | Self::QuestUpdates(_) | Self::Firework(_) => Lane::Reliable,
            Self::Snapshot(_)
            | Self::PlayerMoves(_)
            | Self::ProjectileShot(_)
            | Self::ActorMove(_)
            | Self::MissileLaunch(_)
            | Self::MissileMove(_)
            | Self::PlayerDeath(_)
            | Self::ActorDeath(_)
            | Self::MissileDetonated(_)
            | Self::PlayerHit(_)
            | Self::PlayerFallDamage(_)
            | Self::PlayerBlast(_)
            | Self::ActorHit(_)
            | Self::ActorBeam(_)
            | Self::PlayerStatus(_)
            | Self::CookieCollected(_)
            | Self::HealthPotionCollected(_)
            | Self::MissilesCollected(_)
            | Self::PressurePlate(_)
            | Self::PortalOpened(_)
            | Self::Pong(_) => Lane::Unreliable,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::encode_message;

    // Comfortably under quinn's ~1150-byte datagram limit at the initial MTU.
    const DATAGRAM_BUDGET: usize = 1100;

    fn position() -> Position {
        Position {
            x: 12.5,
            y: 3.0,
            z: -7.25,
        }
    }

    fn barrier_kind_cap() -> u16 {
        u16::try_from(BarrierKindId::MAX.expect("barrier kinds carry no collision-group cap"))
            .expect("barrier kind cap exceeds u16")
    }

    #[test]
    fn unreliable_lane_messages_fit_one_datagram() {
        let messages = [
            ServerMessage::PlayerStatus(SPlayerStatus {
                id: PlayerId(1),
                power_ups: [true; PowerUpKind::COUNT],
                stunned: true,
                held_keys: (0..barrier_kind_cap()).map(BarrierKindId).collect(),
            }),
            ServerMessage::PortalOpened(SPortalOpened {
                shooter: PlayerId(1),
                portal: Portal {
                    pair: PortalPairId(1),
                    end: PortalEnd::A,
                    pos: position(),
                    nx: 0.0,
                    ny: 1.0,
                    nz: 0.0,
                    yaw: 0.5,
                    anchor: None,
                },
            }),
            ServerMessage::PlayerBlast(SPlayerBlast {
                id: PlayerId(1),
                health: Health(10.0),
                vertical_velocity: 7.0,
                velocity_x: 1.0,
                velocity_z: -1.0,
                hit_dir_x: 0.7,
                hit_dir_z: 0.7,
                strength: 0.5,
            }),
            ServerMessage::PlayerDeath(SPlayerDeath {
                id: PlayerId(1),
                pos: position(),
                killer: Some(PlayerId(2)),
                victim_score: -1000,
                killer_score: Some(200),
                explodes: true,
            }),
            ServerMessage::ProjectileShot(SProjectileShot {
                id: PlayerId(1),
                face_yaw: 1.0,
                face_pitch: 0.1,
                pattern: Some("line_5".to_owned()),
            }),
            ServerMessage::ActorBeam(SActorBeam {
                id: ActorId(3),
                target: PlayerId(1),
                duration_secs: 2.0,
            }),
        ];
        for message in &messages {
            assert_eq!(message.lane(), Lane::Unreliable, "{message:?}");
            let len = encode_message(message).expect("message failed to encode").len();
            assert!(len < DATAGRAM_BUDGET, "{message:?} encodes to {len} bytes");
        }
    }

    #[test]
    fn reliable_lane_carries_bootstrap_state_and_text() {
        assert_eq!(ServerMessage::Feed(SFeed { spans: Vec::new() }).lane(), Lane::Reliable);
        assert_eq!(ServerMessage::Firework(SFirework { seed: 7 }).lane(), Lane::Reliable);
        assert_eq!(
            ServerMessage::QuestUpdates(SQuestUpdates { updates: Vec::new() }).lane(),
            Lane::Reliable
        );
        assert_eq!(
            ClientMessage::Login(CLogin { name: String::new() }).lane(),
            Lane::Reliable
        );
        assert_eq!(
            ClientMessage::Ping(CPing { timestamp_nanos: 0 }).lane(),
            Lane::Unreliable
        );
    }

    #[test]
    fn actions_are_reliable_and_input_is_not() {
        assert_eq!(ClientMessage::Jump(CJump {}).lane(), Lane::Reliable);
        let input = PlayerInput {
            move_intent: PlayerMoveIntent::Idle,
            face_yaw: 0.0,
        };
        assert_eq!(
            ClientMessage::Move(CMove { seq: 1, input, hops: 0 }).lane(),
            Lane::Unreliable
        );
        assert_eq!(
            ClientMessage::Ping(CPing { timestamp_nanos: 0 }).lane(),
            Lane::Unreliable
        );
    }

    #[test]
    fn sequence_comparison_wraps() {
        assert!(sequence_is_newer(2, 1));
        assert!(!sequence_is_newer(1, 2));
        assert!(!sequence_is_newer(5, 5));
        assert!(sequence_is_newer(0, u32::MAX));
        assert!(!sequence_is_newer(u32::MAX, 0));
    }

    #[test]
    fn hotel_sized_snapshot_takes_the_stream_carrier() {
        let player = |i: u32| {
            (
                PlayerId(i),
                Player {
                    name: format!("Player {i}"),
                    movement: PlayerMovementState::new(position(), PlayerMoveIntent::Idle, 0.0, 0.0),
                    health: Health(500.0),
                    score: 0,
                    power_ups: [false; PowerUpKind::COUNT],
                    stunned: false,
                    held_keys: Vec::new(),
                    missiles: 0,
                    portal_access: PortalAccess::None,
                    hops: 0,
                },
            )
        };
        let actor = |i: u32| {
            (
                ActorId(i),
                Actor {
                    kind: "sentry".to_owned(),
                    movement: ActorMovementState {
                        pos: position(),
                        move_intent: ActorMoveIntent::Idle,
                        vertical_velocity: 0.0,
                    },
                    face_yaw: 0.0,
                    health: Health(1000.0),
                },
            )
        };
        let item = |i: u32| {
            (
                ItemId(i),
                Item {
                    item_type: ItemType::Cookie,
                    pos: position(),
                },
            )
        };
        let snapshot = ServerMessage::Snapshot(SSnapshot {
            tick: 1,
            players: (0..4).map(player).collect(),
            actors: (0..24).map(actor).collect(),
            spawning_actors: Vec::new(),
            items: (0..74).map(item).collect(),
            missiles: Vec::new(),
            plates: PlateState::default(),
            quests: Vec::new(),
            locked_plate_purposes: Vec::new(),
            rain_intensity: 0.0,
            lighting: LightingBlend {
                from: "bright".to_owned(),
                to: "bright".to_owned(),
                blend: 0.0,
            },
            portals: Vec::new(),
        });
        let len = encode_message(&snapshot).expect("snapshot failed to encode").len();
        assert!(len > DATAGRAM_BUDGET, "hotel-sized snapshot encodes to {len} bytes");
    }
}
