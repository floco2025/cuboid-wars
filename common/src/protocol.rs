// Messages exchanged by client and server.
// Names starting with C are sent by the client; names starting with S are
// sent by the server. For example: client CLogin -> server SInit.
//
// Delivery: two QUIC lanes
//
// A lane controls how a message travels:
// * Reliable: messages arrive in send order while the connection stays open.
//   Lost data is sent again, which can delay the messages behind it.
// * Unreliable: messages may be lost or arrive out of order. Receivers must
//   cope with both. Small messages use datagrams (individual packets); larger
//   messages use separate streams. The transport never discards them itself.
//
// The lanes are independent: an unreliable update can arrive before an
// earlier reliable message. Ordering rules belong to the receiving code.
//
// Each message's role determines its default lane, assigned by `lane()` below.
// Pending portal crossings temporarily put `CMove` on the reliable lane;
// that exchange is described below. `common/src/network.rs` handles delivery
// using the supplied lane and knows nothing about gameplay.
//
// Message roles
//
// 1. Bootstrap (reliable): starting the connection.
//
//    The client sends `CLogin`; the server replies with `SInit`, containing
//    the player's ID, gameplay settings, and map. This happens once per
//    connection. `SInit` is the server's first reliable message.
//
//    An unreliable message may arrive before `SInit`. The client discards
//    everything until it receives `SInit`, so it needs no startup buffer.
//    The server accepts gameplay messages after handling `CLogin`; an update
//    that arrives before login is dropped with a warning. There is no extra
//    "client ready" message.
//
// 2. State (unreliable): the current picture, sent repeatedly.
//
//    `SSnapshot` lists the players, actors, items, missiles, and shared world
//    state, such as plates, quests, weather, and portals. It is sent at
//    `SNAPSHOT_HZ`. The client uses the entity lists to create missing entities
//    and remove absent ones. `spawning_actors` lets it show an actor's arrival
//    effect during the warning period before that actor exists.
//
//    `SPlayerMoves` sends every active player's movement each server tick.
//    Both it and `SSnapshot` carry a `tick`: the server simulation step that
//    produced the update. The client tracks the newest tick separately for
//    each stream and ignores older updates. It also keeps its own estimate
//    of the server's tick in `ServerTick`.
//
//    The client sends its own movement in `CMove` every tick, even when idle:
//    the whole `PlayerMovementState` its step produced, with a sequence
//    number (`seq`) identifying that client update. A lost report is
//    replaced by the next; nothing is retransmitted. The server applies the
//    reported intent and facing, simulates the tick, and adopts the reported
//    state whole (position, vertical velocity, momentum, knockback, support)
//    when the positions agree within `PLAYER_MOVEMENT_TRUST_DISTANCE` on
//    every axis (`MovementDivergence`). Otherwise it keeps its simulated
//    position; intent and facing were applied either way.
//
//    `PlayerMove.move_seq` identifies the report processed on that server
//    tick. The owning client compares the reply with its saved position for
//    that sequence: small differences do nothing; large differences snap.
//    This pairing also helps synchronize the client's clock. Without a fresh
//    report the server keeps simulating, but sends no `move_seq`, so the
//    client cannot compare a later server position with an earlier report.
//
//    Remote players smooth corrections between updates. Actors and missiles
//    also use snapshots and movement cues to correct their local simulation;
//    they have no client movement reports to match.
//
//    Projectiles are short-lived and numerous, so snapshots omit them.
//    `SProjectileShot` tells clients to simulate the visible shot; the server
//    still decides hits and deaths. Missiles last longer and change course,
//    so they remain in snapshots.
//
// 3. Cues (unreliable): prompt feedback before the next state update.
//
//    A lost cue can cost a sound, animation, or delay, but later state updates
//    keep the game state correct. Cues serve three purposes:
//    * Earlier updates: `SActorMove`, `SMissileMove`, and `SMissileLaunch` let
//      clients predict motion before the next snapshot. `SActorBeam` reports
//      beam changes; snapshots also carry the beam state.
//    * One-time feedback: `SPlayerStatus` can play a pickup sound when an item
//      is collected. Repeated snapshots keep the inventory correct without
//      playing the sound again. `SGoldCollected` does the same for gold.
//    * Details absent from snapshots: `SPlayerHit` carries the hit direction
//      for camera shake. Death cues supply the death position and effects;
//      the next snapshot still confirms that the entity is gone.
//
//    `CPing` and `SPong` use this lane to measure round-trip time (RTT).
//
// 4. Events (reliable): information a later snapshot cannot replace.
//
//    Examples sent by the server:
//    * `SQuestUpdates`: the recipient's quest progress. Each update contains
//      the complete state of the affected quest. Group quest progress also
//      appears in snapshots.
//    * `SFeed`: a chat, announcement, or admin reply, with text and styling
//      chosen by the server. It goes to everyone or selected recipients.
//    * `SFirework`: the seed that makes clients play the same firework show.
//
//    Client actions also use this lane: shots (`CProjectileShot`,
//    `CMissileShot`, `CPortalShot`) and console submissions (`CAdmin`,
//    `CChat`). A later movement report cannot repeat a lost action. A jump is
//    not an action: the report after it carries the vertical velocity it
//    produced.
//
//    An impulse the server applies to a player's own movement (`SPlayerBlast`)
//    is an event too: the next accepted report replaces the server's copy of
//    it, so the victim's client is the only place the impulse survives.
//
//    Portal crossings use a reliable exchange too. The owning client crosses
//    immediately and sends `CPortalCross` instead of that tick's `CMove`, with
//    the movement states before and after crossing. The server compares the
//    entrance position after its corresponding movement step, using the same
//    distance limit as ordinary movement. If accepted, it adopts the exit
//    state. It never independently crosses a player.
//
//    `SPortalCrossed` reports the decision. Acceptance goes to everyone: the
//    owner keeps its current prediction, while observers move the remote
//    player to the exit. The server's two lanes are as independent as the
//    client's, so the event can arrive after the same tick's `SPlayerMoves`;
//    observers apply it whenever it arrives, since a body that kept solid
//    portal backing cannot be smoothed through the wall. Rejection goes only
//    to the owner, with the server state to snap back to. A snapshot cannot
//    tell the owner whether its crossing was accepted.
//
//    On rejection, both sides discard movement and further crossings that
//    depended on the rejected crossing. The owner snaps back and answers
//    every rejection with `CPortalRecovery` carrying its last sent sequence,
//    whether or not it still holds the crossing: the server accepts no
//    report until the recovery arrives, and only a death would otherwise
//    clear that. The server then accepts fresh reports and discards anything
//    from before the recovery.
//
//    While a crossing awaits confirmation, subsequent `CMove`s use the same
//    reliable lane so they cannot arrive ahead of it: a report that overtook
//    the crossing would advance the sequence cutoff past it, and the server
//    would drop the crossing without a reply. When reports queue up, the
//    server keeps only the latest in each run of ordinary moves, but
//    processes every crossing in order.
//
// Adding messages
//
// Prefer a snapshot field for shared state. Add a cue when timely feedback
// or a one-time effect is needed, and an event when loss cannot be repaired
// by later state. Assign the corresponding lane in `lane()`.
//
// The server gets the sender's `PlayerId` from the connection. Client
// messages omit that ID so a client cannot claim to be another player.

use std::fmt;

use bevy_ecs::prelude::Resource;
use bevy_math::Vec3;
use bincode::{Decode, Encode};

pub use crate::types::*;
use crate::{config::GameplayBootstrap, constants::PLAYER_MOVEMENT_TRUST_DISTANCE};

// ============================================================================
// Client Messages
// ============================================================================

// Client to Server: Login request.
#[derive(Debug, Clone, Encode, Decode)]
pub struct CLogin {
    pub name: String,
}

// Sent after each movement step, before knockback decay; the server compares
// against its corresponding step using the intent in the reported state.
#[derive(Debug, Clone, Encode, Decode)]
pub struct CMove {
    pub seq: u32,
    pub movement: PlayerMovementState,
}

// Reliable crossing boundary: compare the entrance side, then adopt the exit.
#[derive(Debug, Clone, Encode, Decode)]
pub struct CPortalCross {
    pub seq: u32,
    pub entrance: PlayerMovementState,
    pub movement: PlayerMovementState,
}

// Sent after applying a rejected crossing; earlier movement is now obsolete.
#[derive(Debug, Clone, Encode, Decode)]
pub struct CPortalRecovery {
    pub seq: u32,
}

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
// answers with `SPortalOpened` or a material-rejection `SPortalFizzled` cue.
// Other placement failures are silent; the client predicts their dry-fire.
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

// Admin command text (e.g. "rain start"). The server checks permission,
// interprets and runs the command, and replies through `SFeed`. Adding a
// command needs no protocol change.
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
    pub items: MapItems,
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

// Periodic world state. Entity lists tell the client what exists;
// cues provide earlier feedback, and later snapshots repair missed updates.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SSnapshot {
    // The server tick whose state this is; the client ignores a snapshot
    // older than the last one it applied.
    pub tick: u32,
    pub players: Vec<(PlayerId, Player)>,
    pub actors: Vec<(ActorId, Actor)>,
    pub actors_peaceful: bool,
    // Reserved spawns still in their warning window. An id moves from here
    // to `actors` in the snapshot where the actor materializes.
    pub spawning_actors: Vec<(ActorId, SpawningActor)>,
    pub items: Vec<(ItemId, Item)>,
    // Missiles last long enough to need position updates. This list repairs
    // missed launch and course-change cues.
    pub missiles: Vec<(MissileId, Missile)>,
    // What the pressure plates hold right now: open barrier kinds (the
    // client hides them; the server unions them with each player's
    // `held_keys` for the collision filter) and powered bridge kinds (solid
    // and lit on both sides). Empty on maps with no plates.
    pub plates: PlateState,
    // Unlocked `shared` / `everyone` quests. Completed quests stay listed
    // for the session so late joiners and clients that missed updates catch up.
    pub quests: Vec<QuestGroupStatus>,
    // Plate purposes still locked behind a quest: the plates that solve a
    // quest are inert and hidden until that quest unlocks. Sorted, usually
    // empty.
    pub locked_plate_purposes: Vec<PlatePurpose>,
    // Weather from 0.0 (clear) to 1.0 (full rain). Repeated snapshots keep
    // late joiners and clients that missed updates in sync. Clients smooth
    // the changes when rendering rain.
    pub rain_intensity: f32,
    // Server-driven lighting: which two presets the world is between and
    // how far. Same snapshot rationale as the rain intensity. The client
    // resolves the names against its configured looks and eases toward the
    // blended result.
    pub lighting: LightingBlend,
    // Placed portal ends, sorted by pair and end. This list supplies portals
    // to late joiners and repairs missed `SPortalOpened` cues.
    pub portals: Vec<Portal>,
}

// Movement between snapshots. `tick` orders these updates; each `move_seq`
// identifies the client report processed on that tick. Snapshots determine
// which players exist.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SPlayerMoves {
    pub tick: u32,
    pub moves: Vec<PlayerMove>,
}

#[derive(Debug, Clone, Copy, Encode, Decode)]
pub struct PlayerMove {
    pub id: PlayerId,
    pub movement: PlayerMovementState,
    // Some(seq) only for a report processed this tick; None avoids comparing
    // continued server movement against an earlier client position.
    pub move_seq: Option<u32>,
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

// Missile course change. Sent when the direction changes enough; clients
// continue in a straight line between updates and correct toward this position.
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
    // The next snapshot omits the victim, so this cue supplies the death
    // position. The client snaps here to show the corpse at the server's
    // death spot even when local prediction placed it elsewhere.
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
    pub effect: PlayerDeathEffect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum PlayerDeathEffect {
    Explosion,
    VoidFall,
    GroupRespawn,
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

// Hard-landing damage, sent only to the victim for the health display and
// vertical camera shake. A lethal fall also sends `SPlayerDeath` that tick.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SPlayerFallDamage {
    pub id: PlayerId,
    pub health: Health,
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

// Beam start, retarget, or stop; snapshots carry the same burst state.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SActorBeam {
    pub id: ActorId,
    pub tick: u32,
    pub beam: Option<ActorBeam>,
}

// Player status changed (power-ups, stun, keys, or ammo). The same
// state is also in `SSnapshot`, but this event is the edge trigger that fires
// the associated sounds exactly once at the transition.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct SPlayerStatus {
    pub id: PlayerId,
    // The item just collected when this status change is a pickup; the
    // collector plays the pickup sound and auto-selects the weapon once
    // (`pending_weapon_selection`).
    pub collected: Option<ItemType>,
    // One bool per `PowerUpKind`, indexed by `PowerUpKind::index()`.
    pub power_ups: [bool; PowerUpKind::COUNT],
    pub stunned: bool,
    // Held key inventory. Kept sorted ascending on the server so the encoded
    // bytes are deterministic and the client can change-detect via a single
    // equality test.
    pub held_keys: Vec<BarrierKindId>,
    pub missiles: u32,
}

impl SPlayerStatus {
    #[must_use]
    pub const fn power_up(&self, kind: PowerUpKind) -> bool {
        self.power_ups[kind.index()]
    }
}

// Sent only to the entering player to play a sound when an eraser removes equipment.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SEraserEntered;

// Sent only to the player whose checkpoint changed, for the sound and banner.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SCheckpointReached;

// Player collected gold. Sent only to the collecting player; drives the
// pickup sound AND carries the post-pickup score for snappier HUD reaction.
// The snapshot remains the system of record — this is just an early-arriving
// redundant copy; the next `SSnapshot` will agree.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SGoldCollected {
    pub score: i32,
}

// Player collected a health potion. Sent only to that player for the pickup sound +
// the post-pickup health value, so the HUD updates immediately rather than
// waiting up to a snapshot interval. `SPlayerStatus` carries no health;
// the snapshot's `Player.health` is still the system of record.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SHealthPotionCollected {
    pub health: Health,
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
// portal-gun sound. The snapshot's `portals` list is the system of record.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SPortalOpened {
    pub shooter: PlayerId,
    pub portal: Portal,
}

// Cosmetic impact only; a lost cue costs the fizzle, never portal state.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SPortalFizzled {
    pub shooter: PlayerId,
    pub impact: Portal,
}

// Pong response — server echoes the `CPing` timestamp back unchanged so the
// client can compute RTT from the round trip.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SPong {
    pub timestamp_nanos: u64,
}

// --- Events (delivered; nothing in the snapshot could stand in) ---

// Accepted transitions reach everyone; rejections reach only the crossing player.
// One type keeps this rare reply simple; the owner ignores movement on acceptance.
#[derive(Debug, Clone, Copy, Encode, Decode)]
pub struct SPortalCrossed {
    pub id: PlayerId,
    pub seq: u32,
    pub tick: u32,
    pub accepted: bool,
    pub movement: PlayerMovementState,
}

// Blast result, sent only to the surviving victim. The absolute velocities
// are the blast: the victim's next accepted report replaces the server's
// copy, so the client must apply them itself, and a lost one would be lost
// for good. Health updates the HUD on the damage tick. Direction/strength
// ride along for future feedback use — the client currently plays none
// (the knockback itself is the feedback).
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

// One server-rendered message-feed line. Spans carry semantic styles so the
// client only maps them to its configured presentation.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct SFeed {
    pub spans: Vec<FeedSpan>,
}

// Complete state of one assigned quest. Updates are rare, so repeating the
// display text keeps each update usable without waiting for another message.
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

// Initial assignment and unlocks may include several quests; progress updates
// usually contain one. Sent separately to each affected player because even
// group quests include that player's own progress.
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
    // State
    Move(CMove),
    // Cues
    Ping(CPing),
    // Events
    ProjectileShot(CProjectileShot),
    MissileShot(CMissileShot),
    PortalShot(CPortalShot),
    PortalCross(CPortalCross),
    PortalRecovery(CPortalRecovery),
    Admin(CAdmin),
    Chat(CChat),
}

// All server to client messages. Variants are grouped by role to match the
// struct ordering above; new messages should land in the appropriate group.
// Bincode identifies each variant by its position in this list. Reordering
// changes the wire format; client and server must build from matching source.
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
    ActorHit(SActorHit),
    ActorBeam(SActorBeam),
    PlayerStatus(SPlayerStatus),
    EraserEntered(SEraserEntered),
    CheckpointReached(SCheckpointReached),
    GoldCollected(SGoldCollected),
    HealthPotionCollected(SHealthPotionCollected),
    PressurePlate(SPressurePlate),
    PortalOpened(SPortalOpened),
    PortalFizzled(SPortalFizzled),
    Pong(SPong),
    // Events
    PortalCrossed(SPortalCrossed),
    PlayerBlast(SPlayerBlast),
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

// How far two positions of one body disagree, judged per axis against a
// limit. Displays as the line every rejection and snap logs.
#[derive(Debug, Clone, Copy)]
pub struct MovementDivergence {
    pub delta: Vec3,
    pub limit: f32,
}

impl MovementDivergence {
    #[must_use]
    pub fn between(from: Position, to: Position, limit: f32) -> Self {
        Self {
            delta: Vec3::from(to) - Vec3::from(from),
            limit,
        }
    }

    #[must_use]
    pub fn within_limit(&self) -> bool {
        self.delta.is_finite() && self.delta.abs().max_element() < self.limit
    }
}

impl fmt::Display for MovementDivergence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let magnitudes = self.delta.abs();
        let axis = if magnitudes.x >= magnitudes.y && magnitudes.x >= magnitudes.z {
            "x"
        } else if magnitudes.y >= magnitudes.z {
            "y"
        } else {
            "z"
        };
        write!(
            f,
            "worst axis {axis}: {:.2} m (limit {:.2}; Δ x={:.2}, y={:.2}, z={:.2})",
            magnitudes.max_element(),
            self.limit,
            self.delta.x,
            self.delta.y,
            self.delta.z
        )
    }
}

impl PlayerMovementState {
    // The trust rule: the server adopts a report, and the owning client keeps
    // its prediction, only while the two positions agree this closely.
    #[must_use]
    pub fn divergence_from(&self, other: &Self) -> MovementDivergence {
        MovementDivergence::between(other.pos, self.pos, PLAYER_MOVEMENT_TRUST_DISTANCE)
    }
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
            | Self::ProjectileShot(_)
            | Self::MissileShot(_)
            | Self::PortalShot(_)
            | Self::PortalCross(_)
            | Self::PortalRecovery(_)
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
            Self::Init(_)
            | Self::PortalCrossed(_)
            | Self::PlayerBlast(_)
            | Self::Feed(_)
            | Self::QuestUpdates(_)
            | Self::Firework(_) => Lane::Reliable,
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
            | Self::ActorHit(_)
            | Self::ActorBeam(_)
            | Self::PlayerStatus(_)
            | Self::EraserEntered(_)
            | Self::CheckpointReached(_)
            | Self::GoldCollected(_)
            | Self::HealthPotionCollected(_)
            | Self::PressurePlate(_)
            | Self::PortalOpened(_)
            | Self::PortalFizzled(_)
            | Self::Pong(_) => Lane::Unreliable,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{network::encode_message, protocol::CarrierId};

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
            ServerMessage::EraserEntered(SEraserEntered),
            ServerMessage::CheckpointReached(SCheckpointReached),
            ServerMessage::PlayerStatus(SPlayerStatus {
                collected: Some(ItemType::SpeedPowerUp),
                id: PlayerId(1),
                power_ups: [true; PowerUpKind::COUNT],
                stunned: true,
                held_keys: (0..barrier_kind_cap()).map(BarrierKindId).collect(),
                missiles: 0,
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
                    carrier: CarrierId::WORLD,
                },
            }),
            ServerMessage::PlayerDeath(SPlayerDeath {
                id: PlayerId(1),
                pos: position(),
                killer: Some(PlayerId(2)),
                victim_score: -1000,
                killer_score: Some(200),
                effect: PlayerDeathEffect::Explosion,
            }),
            ServerMessage::ProjectileShot(SProjectileShot {
                id: PlayerId(1),
                face_yaw: 1.0,
                face_pitch: 0.1,
                pattern: Some("line_5".to_owned()),
            }),
            ServerMessage::ActorBeam(SActorBeam {
                id: ActorId(3),
                tick: u32::MAX,
                beam: Some(ActorBeam {
                    target: PlayerId(1),
                    started_tick: u32::MAX,
                    remaining_secs: 2.0,
                }),
            }),
            ServerMessage::ActorBeam(SActorBeam {
                id: ActorId(3),
                tick: 0,
                beam: None,
            }),
        ];
        for message in &messages {
            assert_eq!(message.lane(), Lane::Unreliable, "{message:?}");
            let len = encode_message(message).expect("message failed to encode").len();
            assert!(len < DATAGRAM_BUDGET, "{message:?} encodes to {len} bytes");
        }
    }

    #[test]
    fn reliable_lane_carries_bootstrap_events_and_text() {
        let movement = PlayerMovementState::new(position(), PlayerMoveIntent::Idle, 0.0, 0.0);
        assert_eq!(ServerMessage::Feed(SFeed { spans: Vec::new() }).lane(), Lane::Reliable);
        assert_eq!(ServerMessage::Firework(SFirework { seed: 7 }).lane(), Lane::Reliable);
        assert_eq!(
            ServerMessage::QuestUpdates(SQuestUpdates { updates: Vec::new() }).lane(),
            Lane::Reliable
        );
        assert_eq!(
            ServerMessage::PortalCrossed(SPortalCrossed {
                id: PlayerId(1),
                seq: 1,
                tick: 1,
                accepted: true,
                movement,
            })
            .lane(),
            Lane::Reliable
        );
        assert_eq!(
            ServerMessage::PlayerBlast(SPlayerBlast {
                id: PlayerId(1),
                health: Health(10.0),
                vertical_velocity: 7.0,
                velocity_x: 1.0,
                velocity_z: -1.0,
                hit_dir_x: 0.7,
                hit_dir_z: 0.7,
                strength: 0.5,
            })
            .lane(),
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
    fn actions_and_crossings_are_reliable_and_movement_is_not() {
        let movement = PlayerMovementState::new(position(), PlayerMoveIntent::Idle, 0.0, 0.0);
        assert_eq!(
            ClientMessage::PortalCross(CPortalCross {
                seq: 1,
                entrance: movement,
                movement,
            })
            .lane(),
            Lane::Reliable
        );
        assert_eq!(
            ClientMessage::PortalRecovery(CPortalRecovery { seq: 1 }).lane(),
            Lane::Reliable
        );
        assert_eq!(ClientMessage::Move(CMove { seq: 1, movement }).lane(), Lane::Unreliable);
        assert_eq!(
            ClientMessage::Ping(CPing { timestamp_nanos: 0 }).lane(),
            Lane::Unreliable
        );
    }

    #[test]
    fn movement_trust_is_finite_and_strictly_per_axis() {
        let limit = PLAYER_MOVEMENT_TRUST_DISTANCE;
        let divergence = |delta| MovementDivergence { delta, limit };
        assert!(divergence(Vec3::splat(limit - 0.01)).within_limit());
        for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
            for sign in [-1.0, 1.0] {
                assert!(divergence(axis * sign * (limit - 0.01)).within_limit());
                assert!(!divergence(axis * sign * limit).within_limit());
                assert!(!divergence(axis * sign * (limit + 0.01)).within_limit());
            }
        }
        assert!(!divergence(Vec3::splat(f32::NAN)).within_limit());
        assert!(!divergence(Vec3::splat(f32::INFINITY)).within_limit());
    }

    #[test]
    fn divergence_reports_the_dominant_axis() {
        let describe = |delta| MovementDivergence { delta, limit: 5.0 }.to_string();
        assert!(describe(Vec3::new(-3.0, 1.0, 2.0)).starts_with("worst axis x: 3.00 m (limit 5.00;"));
        assert!(describe(Vec3::new(1.0, -3.0, 2.0)).starts_with("worst axis y: 3.00 m"));
        assert!(describe(Vec3::new(1.0, 2.0, -3.0)).starts_with("worst axis z: 3.00 m"));
        assert!(describe(Vec3::splat(2.0)).starts_with("worst axis x"));
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
                },
            )
        };
        let actor = |i: u32| {
            (
                ActorId(i),
                Actor {
                    anchor: None,
                    beam: None,
                    kind: "bruiser".to_owned(),
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
                    item_type: ItemType::Gold,
                    carrier: CarrierId::WORLD,
                    pos: position(),
                },
            )
        };
        let snapshot = ServerMessage::Snapshot(SSnapshot {
            tick: 1,
            players: (0..4).map(player).collect(),
            actors: (0..24).map(actor).collect(),
            actors_peaceful: false,
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
