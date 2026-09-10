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
// `common/src/network.rs` handles delivery
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
//    `network.snapshot_hz`. The client uses the entity lists to create missing entities
//    and remove absent ones. `spawning_actors` lets it show an actor's arrival
//    effect during the warning period before that actor exists.
//
//    `SPlayerMoves` and `SActorMoves` send player and actor movement at
//    `network.update_hz`. These messages and `SSnapshot` carry the server
//    `tick` that produced them. Player movement and snapshots have separate
//    ordering guards; actor samples compare ticks across both sources.
//    The client estimates the server's tick in `ServerTick`. `SInit` supplies
//    `network.server_hz` for that clock and both fixed simulation loops.
//
//    The client sends `CMove` at the same configured rate, even when idle:
//    the whole `PlayerMovementState` its step produced, with a sequence
//    number (`seq`) identifying that client update. A lost report is
//    replaced by the next; nothing is retransmitted. The server adopts the
//    reported state whole (position, intent, facing, vertical velocity,
//    momentum, knockback, support), without simulating player movement.
//    Fresh, finite reports for the current body are always accepted.
//
//    A rider's position is relative to `movement.carrier`; everyone else uses
//    the world carrier. The server retains these coordinates and places the
//    body with the carrier's current pose. Observers interpolate the relative
//    positions using the same rendered carrier pose as the platform itself.
//    Changing carriers sends a report immediately, including boarding and takeoff.
//
//    `seq` advances every client simulation step, including unsent steps, so
//    observers can time movement samples independently of packet arrival.
//    The server repeats the latest report until another arrives. Observers
//    ignore repeated sequences and interpolate between buffered samples;
//    they hold the newest position when the buffer runs dry.
//
//    Portal crossings send a report immediately. `portal_crossing` counts
//    crossings within this body and repeats in every subsequent report, so
//    observers cut across a crossing even if its first report is lost.
//
//    Each player's `generation` advances on respawn or forced relocation.
//    Reports and body-bound cues apply only to that generation. `SPlayerRelocated`
//    reliably supplies a complete player at its spawn or relocation; snapshots
//    can establish it first. Applying a generation twice never moves the owner
//    again. Older generations cannot undo a relocation or resurrect a dead body.
//
//    Actors interpolate buffered server samples, including facing and support;
//    their positions use the actor's carrier frame. Only snapshots establish
//    actor presence. Clients hold the newest sample when the buffer runs dry
//    and never simulate actor physics. `SActorMoves.tick` identifies every
//    sample in its batch, so actors need no separate sequence number.
//
//    Only the shooter simulates a missile. `CMissileMoves` sends its state at
//    `network.update_hz`; the server adopts fresh samples and relays them
//    immediately in `SMissileMoves`. Each missile's `seq` counts owner flight
//    steps, including unsent ones. Snapshots repeat that sequence and state.
//    Observers interpolate those samples and hold when the buffer runs dry;
//    neither the server nor observers simulate missile flight or collision.
//
//    Projectiles are short-lived and numerous, so snapshots omit them.
//    `CProjectileShot` and its `SProjectileShot` relay are immediate unreliable
//    cues: one eye origin, aim, and numeric pattern per volley.
//    Every client simulates its visible bullets; only the shooter reports hits.
//    Missing volleys cost cosmetics, never damage. Every received volley starts
//    at its supplied origin with a full lifetime, regardless of arrival order.
//    Missiles last longer and change course, so they remain in snapshots.
//
// 3. Cues (unreliable): prompt feedback before the next state update.
//
//    A lost cue can cost a sound, animation, or delay, but later state updates
//    keep the game state correct. Cues serve three purposes:
//    * Earlier updates: `SActorBeam` reports beam changes; snapshots also
//      carry the beam state.
//    * One-time feedback: `SPlayerStatus` can play a pickup sound when an item
//      is collected. Repeated snapshots keep the inventory correct without
//      playing the sound again. `SGoldCollected` does the same for gold.
//    * Details absent from snapshots: `SPlayerHit` carries the hit direction
//      for camera shake. Death cues supply the death position and effects;
//      the next snapshot still confirms that the entity is gone.
//
//    `CPing` and `SPong` measure round-trip time (RTT); the pong also carries
//    the server tick for clock synchronization.
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
//    * `SPlayerRelocated`: establishes a body before reliable impulses for it;
//      snapshots may supply it earlier but cannot guarantee that ordering.
//
//    Client actions also use this lane: shots (`CMissileShot`, `CPortalShot`)
//    and console submissions (`CAdmin`, `CChat`). A later movement report
//    cannot repeat a lost action. A jump is
//    not an action: the report after it carries the vertical velocity it
//    produced.
//
//    `CProjectileHit` reports one bullet's victim and impact direction, once.
//    Reliable delivery needs no volley record or bullet ID on the server:
//    hits work even if the cosmetic volley was lost or the shooter has died.
//    The server applies damage, scoring, and death; player victims are bound
//    to their body generation. Visible bullets finish their lifetime after a
//    shooter disconnects, but that shooter can no longer report damage.
//
//    `CMissileShot` carries the firing client's launch geometry and lock.
//    The server checks ammo and the body generation, allocates an ID, and
//    reliably broadcasts `SMissileLaunch`; this starts the owner's flight.
//    `CMissileDetonated` reliably reports the blast position and detected
//    victims, falloff, and impulse directions. The server applies each missile
//    once, using its damage/knockback tuning and current victim generations,
//    without repeating collision or blast-visibility queries. Detonation
//    reports survive lost movement packets and shooter death or respawn;
//    disconnect removes the shooter's missiles. `SMissileDetonated` is reliable
//    so snapshot removal cannot lose the explosion effect. Snapshot ordering
//    cannot reposition an owner's flight or resurrect a detonated missile.
//
//    `CPortalShot` carries the client's placement or material-fizzle impact
//    in carrier coordinates, bound to its body generation. The server checks
//    ownership, equipment, cooldown, and overlap with current portals, then
//    broadcasts the accepted result. Geometry, aim, and material checks run
//    only on the firing client; all clients install accepted portals from
//    `SPortalOpened` or snapshots.
//
//    `CPlayerMovementEvent` reports landing impact speeds, crushing, falls out
//    of the world, and equipment erasure. These events are ordered
//    by the reliable lane, independently of movement sequence cutoffs; a later
//    movement report cannot cancel an event or repeat it. Impact positions
//    locate death explosions, never reposition a body. The server applies damage,
//    equipment, and lifecycle rules, dropping events for other generations.
//
//    `SPlayerKnockback` supplies an additive impulse to the surviving owner.
//    The client adds it once to its current motion, clamping the planar sum;
//    the server keeps its reported movement copy until the next report.
//
// Adding messages
//
// Prefer a snapshot field for shared state. Add a cue when timely feedback
// or a one-time effect is needed, and an event when loss cannot be repaired
// by later state. Assign the corresponding lane in `lane()`.
//
// The server gets the sender's `PlayerId` from the connection. Client
// messages omit that ID so a client cannot claim to be another player.

use bevy_ecs::prelude::Resource;
use bincode::{Decode, Encode};

use crate::config::{GameplayBootstrap, NetworkConfig};
pub use crate::math::sequence_is_newer;
pub use crate::types::*;

// ============================================================================
// Client Messages
// ============================================================================

// Client to Server: Login request.
#[derive(Debug, Clone, Encode, Decode)]
pub struct CLogin {
    pub name: String,
}

// Sent after movement and portal traversal, before knockback decay.
#[derive(Debug, Clone, Encode, Decode)]
pub struct CMove {
    pub generation: PlayerGeneration,
    pub seq: u32,
    pub portal_crossing: u32,
    pub movement: PlayerMovementState,
}

// Reliable events are independent of movement sequence cutoffs and never reposition a body.
#[derive(Debug, Clone, Encode, Decode)]
pub struct CPlayerMovementEvent {
    pub generation: PlayerGeneration,
    pub event: PlayerMovementEvent,
}

#[derive(Debug, Clone, Encode, Decode)]
pub enum PlayerMovementEvent {
    Landed {
        // A lethal impact needs its explosion position even if movement reports arrive later.
        pos: Position,
        impact_speed: f32,
    },
    Crushed {
        // The death explosion needs the contact position, which movement reports may not yet contain.
        pos: Position,
    },
    FellOutOfWorld,
    EraseEquipment,
}

#[derive(Debug, Clone, Copy, Encode, Decode)]
pub struct CProjectileShot {
    pub origin: Position,
    pub face_yaw: f32,
    pub face_pitch: f32,
    // 0 is single shot; 1 onwards index the ordered allowed_patterns list.
    pub pattern: u8,
}

#[derive(Debug, Clone, Copy, Encode, Decode)]
pub struct CProjectileHit {
    pub target: HitTarget,
    pub direction: [f32; 2],
}

// The firing client resolves the launch geometry and lock; acceptance starts its flight.
#[derive(Debug, Clone, Encode, Decode)]
pub struct CMissileShot {
    pub generation: PlayerGeneration,
    pub target: Option<HomingTarget>,
    pub movement: MissileMovementState,
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct CMissileMoves {
    pub moves: Vec<MissileMove>,
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct MissileMove {
    pub id: MissileId,
    // Owner simulation steps, including steps that sent no update.
    pub seq: u32,
    pub movement: MissileMovementState,
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct CMissileDetonated {
    pub id: MissileId,
    pub pos: Position,
    pub hits: Vec<MissileBlastHit>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum HitTarget {
    Player { id: PlayerId, generation: PlayerGeneration },
    Actor(ActorId),
}

#[derive(Debug, Clone, Copy, Encode, Decode)]
pub struct MissileBlastHit {
    pub target: HitTarget,
    pub falloff: f32,
    // Planar impulse direction at the victim position seen by the shooter.
    pub direction: [f32; 2],
}

// The client resolves the shot in the hit carrier's frame; the server arbitrates shared overlaps.
#[derive(Debug, Clone, Encode, Decode)]
pub struct CPortalShot {
    pub generation: PlayerGeneration,
    pub result: PortalShotResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Encode, Decode)]
pub enum PortalShotResult {
    Placed(Portal),
    Fizzled(Portal),
}

impl PortalShotResult {
    #[must_use]
    pub const fn portal(self) -> Portal {
        match self {
            Self::Placed(portal) | Self::Fizzled(portal) => portal,
        }
    }
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
    pub network: NetworkConfig,
    pub gameplay: GameplayBootstrap,
    pub map: MapBootstrap,
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct MapBootstrap {
    pub layout: MapLayout,
    pub settings: MapSettings,
    pub items: MapItems,
    pub missile_air_grids: Vec<MissileAirGrid>,
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct MissileAirGrid {
    pub carrier: CarrierId,
    pub cols: i32,
    pub rows: i32,
    pub levels: u8,
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
    // Seeds joining observers and repeats the latest owner samples.
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

// Movement between snapshots. `tick` orders the envelopes; each `seq`
// identifies the latest client sample. Snapshots determine which players exist.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SPlayerMoves {
    pub tick: u32,
    pub moves: Vec<PlayerMove>,
}

#[derive(Debug, Clone, Copy, Encode, Decode)]
pub struct PlayerMove {
    pub id: PlayerId,
    pub generation: PlayerGeneration,
    pub seq: u32,
    pub portal_crossing: u32,
    pub movement: PlayerMovementState,
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct SActorMoves {
    pub tick: u32,
    pub moves: Vec<ActorMove>,
}

#[derive(Debug, Clone, Copy, Encode, Decode)]
pub struct ActorMove {
    pub id: ActorId,
    pub movement: ActorMovementState,
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct SMissileMoves {
    pub moves: Vec<MissileMove>,
}

// --- Cues (ahead of the next snapshot, healed by it) ---

// Cosmetic volley relayed to other clients; only the shooter reports hits.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SProjectileShot {
    pub id: PlayerId,
    pub shot: CProjectileShot,
}

// A player died. Drives the immediate client-side death-state transition
// (overlay + freeze for the dying player, entity teardown for others).
// `SSnapshot`'s next snapshot is the fallback.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SPlayerDeath {
    pub id: PlayerId,
    pub generation: PlayerGeneration,
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
    pub generation: PlayerGeneration,
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
    pub generation: PlayerGeneration,
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
    pub generation: PlayerGeneration,
    // Collected or granted items play the pickup sound and select the weapon once.
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

// Sent to the affected player whenever an eraser removes equipment.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SEquipmentErased;

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
    pub generation: PlayerGeneration,
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
// client can compute RTT and estimate the current server tick.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SPong {
    pub tick: u32,
    pub timestamp_nanos: u64,
}

// --- Events (delivered; nothing in the snapshot could stand in) ---

// Reliable launch delivery starts the shooter's simulation even if no snapshot arrives.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SMissileLaunch {
    pub id: MissileId,
    pub shooter: PlayerId,
    pub tick: u32,
    pub target: Option<HomingTarget>,
    pub movement: MissileMovementState,
}

// Delivers the explosion effect even when a snapshot has already removed the missile.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SMissileDetonated {
    pub id: MissileId,
    pub tick: u32,
    pub pos: Position,
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct SPlayerRelocated {
    pub id: PlayerId,
    pub tick: u32,
    pub player: Player,
}

// Additive blast impulse for the surviving owner; applied once to its current local motion.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SPlayerKnockback {
    pub id: PlayerId,
    pub generation: PlayerGeneration,
    pub health: Health,
    pub impulse: [f32; 3],
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
    MissileMoves(CMissileMoves),
    // Cues
    Ping(CPing),
    ProjectileShot(CProjectileShot),
    // Events
    ProjectileHit(CProjectileHit),
    MissileShot(CMissileShot),
    MissileDetonated(CMissileDetonated),
    PortalShot(CPortalShot),
    PlayerMovementEvent(CPlayerMovementEvent),
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
    ActorMoves(SActorMoves),
    MissileMoves(SMissileMoves),
    // Cues
    ProjectileShot(SProjectileShot),
    PlayerDeath(SPlayerDeath),
    ActorDeath(SActorDeath),
    PlayerHit(SPlayerHit),
    PlayerFallDamage(SPlayerFallDamage),
    ActorHit(SActorHit),
    ActorBeam(SActorBeam),
    PlayerStatus(SPlayerStatus),
    EquipmentErased(SEquipmentErased),
    CheckpointReached(SCheckpointReached),
    GoldCollected(SGoldCollected),
    HealthPotionCollected(SHealthPotionCollected),
    PressurePlate(SPressurePlate),
    PortalOpened(SPortalOpened),
    PortalFizzled(SPortalFizzled),
    Pong(SPong),
    // Events
    MissileLaunch(SMissileLaunch),
    MissileDetonated(SMissileDetonated),
    PlayerRelocated(SPlayerRelocated),
    PlayerKnockback(SPlayerKnockback),
    Feed(SFeed),
    QuestUpdates(SQuestUpdates),
    Firework(SFirework),
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
            | Self::ProjectileHit(_)
            | Self::MissileShot(_)
            | Self::MissileDetonated(_)
            | Self::PortalShot(_)
            | Self::PlayerMovementEvent(_)
            | Self::Admin(_)
            | Self::Chat(_) => Lane::Reliable,
            Self::Move(_) | Self::MissileMoves(_) | Self::ProjectileShot(_) | Self::Ping(_) => Lane::Unreliable,
        }
    }
}

impl ServerMessage {
    #[must_use]
    pub const fn lane(&self) -> Lane {
        match self {
            Self::Init(_)
            | Self::MissileLaunch(_)
            | Self::MissileDetonated(_)
            | Self::PlayerRelocated(_)
            | Self::PlayerKnockback(_)
            | Self::Feed(_)
            | Self::QuestUpdates(_)
            | Self::Firework(_) => Lane::Reliable,
            Self::Snapshot(_)
            | Self::PlayerMoves(_)
            | Self::ProjectileShot(_)
            | Self::ActorMoves(_)
            | Self::MissileMoves(_)
            | Self::PlayerDeath(_)
            | Self::ActorDeath(_)
            | Self::PlayerHit(_)
            | Self::PlayerFallDamage(_)
            | Self::ActorHit(_)
            | Self::ActorBeam(_)
            | Self::PlayerStatus(_)
            | Self::EquipmentErased(_)
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
    use crate::{network::encode_message, physics::CharacterSupport, protocol::CarrierId};

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
            ServerMessage::EquipmentErased(SEquipmentErased),
            ServerMessage::CheckpointReached(SCheckpointReached),
            ServerMessage::PlayerStatus(SPlayerStatus {
                id: PlayerId(1),
                generation: PlayerGeneration(0),
                collected: Some(ItemType::SpeedPowerUp),
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
                generation: PlayerGeneration(0),
                pos: position(),
                killer: Some(PlayerId(2)),
                victim_score: -1000,
                killer_score: Some(200),
                effect: PlayerDeathEffect::Explosion,
            }),
            ServerMessage::ProjectileShot(SProjectileShot {
                id: PlayerId(1),
                shot: CProjectileShot {
                    origin: position(),
                    face_yaw: 1.0,
                    face_pitch: 0.1,
                    pattern: 1,
                },
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
        assert_eq!(ServerMessage::Feed(SFeed { spans: Vec::new() }).lane(), Lane::Reliable);
        assert_eq!(ServerMessage::Firework(SFirework { seed: 7 }).lane(), Lane::Reliable);
        assert_eq!(
            ServerMessage::QuestUpdates(SQuestUpdates { updates: Vec::new() }).lane(),
            Lane::Reliable
        );
        assert_eq!(
            ServerMessage::PlayerKnockback(SPlayerKnockback {
                id: PlayerId(1),
                generation: PlayerGeneration(0),
                health: Health(10.0),
                impulse: [1.0, 7.0, -1.0],
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
    fn movement_is_unreliable() {
        let movement = PlayerMovementState::new(position(), PlayerMoveIntent::Idle, 0.0, 0.0);
        assert_eq!(
            ClientMessage::Move(CMove {
                generation: PlayerGeneration(0),
                seq: 1,
                portal_crossing: 0,
                movement
            })
            .lane(),
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
                    generation: PlayerGeneration(0),
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
                    beam: None,
                    kind: "bruiser".to_owned(),
                    movement: ActorMovementState {
                        pos: position(),
                        carrier: CarrierId::WORLD,
                        move_intent: ActorMoveIntent::Idle,
                        vertical_velocity: 0.0,
                        face_yaw: 0.0,
                        support: CharacterSupport::Ground,
                    },
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

#[cfg(test)]
#[path = "protocol_projectile_tests.rs"]
mod projectile_tests;
