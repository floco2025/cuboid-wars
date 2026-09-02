// Wire protocol between client and server.
//
// Server→client messages fall into six roles (plus a diagnostic channel).
// When adding a new message, pick the smallest role that fits — most shared
// "X changed" things belong in the snapshot, not a new event.
//
// 1. Bootstrap (`SInit`) — sent once at connect with session-level state
//    (`PlayerId`, static `MapLayout`, per-map `MapSettings`).
//
// 2. State snapshot (`SSnapshot`) — the authoritative current state of every
//    player, actor, and item (plus shared world state such as open barrier
//    kinds, group quest status, plate gating, and placed portals), broadcast at
//    `SNAPSHOT_HZ`. Sole vehicle for
//    presence: a player appears in the first `SSnapshot` they show up in and
//    disappears in the first they're absent from. Self-healing — a dropped
//    snapshot is forgiven by the next one. Presence includes pre-presence:
//    `spawning_actors` carries reserved actor spawns during their warning
//    window, so clients render a beam-in ghost before the actor exists.
//
//    Projectiles are the deliberate exception. They are short-lived, fast,
//    and numerous, so they are replicated as shot intents (`SPlayerShot`) rather
//    than snapshot entities. Clients simulate them only for presentation;
//    authoritative hit/death outcomes still come from the server.
//
//    Missiles are NOT that exception: they fly for seconds and steer
//    server-side, so they are full snapshot entities reconciled like actors
//    (`SMissileLaunch` / `SMissileMove` are the latency cues).
//
// 3. Real-time intent — movement prediction inputs (`SPlayerMove`,
//    `SPlayerJump`, `SPlayerShot`, `SActorMove`, `SMissileMove`,
//    `SMissileLaunch`) that must arrive faster than snapshot cadence so
//    clients can dead-reckon between snapshots. Broadcast on change.
//
// 4. One-shot cues — short messages that fire at the moment of a discrete
//    state change in the *shared* world. They sit alongside the snapshot,
//    not replacing it, and exist only when the snapshot alone can't carry
//    the cue, which is one of:
//      * Sub-tick latency matters. Camera shake from `SPlayerHit` needs to
//        land on the impact frame, not 1–2 ticks later.
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
// 5. Per-client state events — durable per-player state that has no place in
//    the world snapshot because other clients don't need it. Unicast to the
//    affected player only. Unlike one-shot cues these install lasting client
//    state (e.g. an active quest's announcement text); the client treats
//    receipt as authoritative until a follow-up message updates it. There
//    is no snapshot-side fallback — recovery from packet loss is QUIC's
//    job, not the protocol's. Used today for quest assignment / progress /
//    completion (`SQuestsAssigned`, `SQuestProgress`, `SQuestCompleted`).
//    Group quest state (pooled progress, players done, completion) is world
//    state and rides the snapshot instead; `SQuestCompleted` reaches every
//    player for group quests.
//
// 6. Feed lines (`SFeed`) — server-authored, human-readable lines for the
//    client's message feed (kills, pickups, quest completions, admin
//    actions, chat). The server sends final text spans with semantic styles;
//    the client only maps those styles to colors. Public lines can target
//    everyone or everyone except one player; admin replies target the issuer.
//    Ephemeral like cues — a dropped line costs only the text — and never a
//    source of state.
//
// `CPing` / `SPong` are a separate diagnostic channel for RTT measurement.
//
// The server supplies the authenticated `PlayerId` from its transport; keeping
// that ID out of the wire payload prevents clients from choosing their own
// identity.

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

// Client to Server: Local player's steady-state input — movement intent plus
// facing, committed together whenever either changes enough.
#[derive(Debug, Clone, Encode, Decode)]
pub struct CMove {
    pub move_intent: PlayerMoveIntent,
    pub face_yaw: f32, // radians - direction the player is facing
}

// Client to Server: One-shot jump request.
#[derive(Debug, Clone, Encode, Decode)]
pub struct CJump {}

// Client to Server: Shot fired.
#[derive(Debug, Clone, Encode, Decode)]
pub struct CShot {
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
// Ordered by role: bootstrap → snapshot → real-time intent → one-shot cues
// → per-client state events → diagnostic. Matches the protocol-model doc
// comment at the top of this file.

// --- Bootstrap ---

// Initial connection acknowledgment with assigned player ID + map layout.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SInit {
    pub id: PlayerId,
    pub portal_access: PortalAccess,
    pub map_layout: MapLayout,
    pub map_settings: MapSettings,
    // Blast radii (m) from the server's combat config, so explosion VFX can
    // telegraph the true danger area: per actor kind (sorted by kind for
    // deterministic encoding), a dying player, a missile.
    pub actor_blast_radii: Vec<(String, f32)>,
    pub player_blast_radius: f32,
    pub missile_blast_radius: f32,
    // Max health from the same config, so health bars have a denominator.
    pub player_max_health: f32,
    pub actor_max_health: Vec<(String, f32)>,
    // Barrier kinds this map places a key for (sorted), so the HUD shows a
    // key slot only where one can be filled.
    pub key_kinds: Vec<BarrierKindId>,
}

// --- Snapshot ---

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
    // In-flight missiles. Unlike projectiles, missiles ARE snapshot entities:
    // they fly for seconds and steer server-side, so presence and position
    // self-heal here while `SMissileMove` carries course changes.
    pub missiles: Vec<(MissileId, Missile)>,
    // Barrier kinds currently fully open (pressure-plate threshold met).
    // Empty in v1 maps with no plates. Client hides matching barriers; server
    // unions this with each player's `held_keys` for the collision filter.
    pub open_barrier_kinds: Vec<BarrierKindId>,
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

// --- Real-time intent (sub-tick latency for prediction) ---

// Player input change (movement intent + facing) for client-side prediction
// of remote players.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SPlayerMove {
    pub id: PlayerId,
    pub movement: PlayerMovementState,
}

// Player started a jump with authoritative vertical velocity. Same payload as
// `SPlayerMove`, different contract: this is the one message allowed to
// overwrite the remote player's simulated vertical velocity (the move stream
// never touches it — its value would be stale mid-flight).
#[derive(Debug, Clone, Encode, Decode)]
pub struct SPlayerJump {
    pub id: PlayerId,
    pub movement: PlayerMovementState,
}

// Player fired a shot. Projectile entities are intentionally not carried in
// `SSnapshot`: clients spawn and simulate them for presentation, while the
// server runs its own projectile simulation for authoritative hit logic.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SPlayerShot {
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
pub struct SMissileDeath {
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

// Player collected a health potion. Unicast one-shot for the pickup sound +
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

// Admin `/firework`: play the client-side firework show. Pure presentation —
// the server broadcasts the seed and forgets; every client derives the same
// choreography from it, so all clients see the same show.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SFirework {
    pub seed: u64,
}

// A portal end was placed or moved. Latency cue for the placement visual
// and sound — and for keeping every client's portal geometry fresh: portal
// crossings are not messaged at all, each client simulates every player's
// crossings from the shared geometry, so a placement must reach observers
// quickly. The snapshot's `portals` list is the system of record.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SPortalOpened {
    pub portal: Portal,
}

// --- Feed lines (server-authored message feed) ---

// One server-rendered message-feed line. Spans carry semantic styles so the
// client only maps them to its configured presentation.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct SFeed {
    pub spans: Vec<FeedSpan>,
}

// --- Per-client state events (private, durable) ---

// One quest in an `SQuestsAssigned` batch. Carries display strings inline so
// the client never needs a separate quest catalog: `title` is the short panel
// label, `description` the longer announcement body. `threshold` is the
// progress denominator; `status` is a complete initial view so assignment
// remains correct when it races the group snapshot on another QUIC stream.
#[derive(Debug, Clone, Encode, Decode)]
pub struct NewQuest {
    pub id: QuestId,
    pub title: String,
    pub description: String,
    pub threshold: u32,
    pub status: QuestInitialStatus,
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

// Quest completed — unicast to the player for `individual` quests, sent to
// every logged-in player for group quests. Marks the quest done in the
// client's panel and fires the completion banner.
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
    Move(CMove),
    Jump(CJump),
    Shot(CShot),
    MissileShot(CMissileShot),
    PortalShot(CPortalShot),
    Ping(CPing),
    Admin(CAdmin),
    Chat(CChat),
}

// All server to client messages. Variants are grouped by role to match the
// struct ordering above; new messages should land in the appropriate group.
// Note: bincode encodes the discriminant by position, so reordering touches
// the wire format — fine for an in-dev workspace where server and client
// always build from the same source.
#[derive(Debug, Clone, Encode, Decode)]
pub enum ServerMessage {
    // Bootstrap
    Init(SInit),
    // Snapshot
    Snapshot(SSnapshot),
    // Real-time intent
    PlayerMove(SPlayerMove),
    PlayerJump(SPlayerJump),
    PlayerShot(SPlayerShot),
    ActorMove(SActorMove),
    MissileLaunch(SMissileLaunch),
    MissileMove(SMissileMove),
    // One-shot cues
    PlayerDeath(SPlayerDeath),
    ActorDeath(SActorDeath),
    MissileDeath(SMissileDeath),
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
    Firework(SFirework),
    PortalOpened(SPortalOpened),
    // Feed lines
    Feed(SFeed),
    // Per-client state events
    QuestsAssigned(SQuestsAssigned),
    QuestProgress(SQuestProgress),
    QuestCompleted(SQuestCompleted),
    // Diagnostic
    Pong(SPong),
}
