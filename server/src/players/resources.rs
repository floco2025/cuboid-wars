use std::collections::{BTreeMap, HashMap};

use bevy::prelude::*;
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    config::{ActorRespawnScope, PlayerRespawnMode, PowerUpsConfig, RespawnConfig},
    network::ServerToClient,
};
use common::protocol::{
    BarrierKindId, CMove, FaceYaw, Health, ItemType, Player, PlayerId, PlayerMarker, PlayerMoveIntent,
    PlayerMovementState, PortalAccess, Position, PowerUpKind, QuestId, QuestScope, SPlayerStatus,
};

use super::{CheckpointId, PlayerCheckpoint, PlayerFallState, PlayerMovementPath, PowerUpState};

pub type PlayerStateQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static Position,
        &'static PlayerMoveIntent,
        &'static FaceYaw,
        &'static Health,
    ),
    With<PlayerMarker>,
>;

#[derive(Resource)]
pub struct Invincibility(pub bool);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerQuestState {
    Individual { progress: u32 },
    Shared,
    Everyone { progress: u32 },
}

impl PlayerQuestState {
    #[must_use]
    pub const fn new(scope: QuestScope, progress: u32) -> Self {
        match scope {
            QuestScope::Individual => Self::Individual { progress },
            QuestScope::Shared => Self::Shared,
            QuestScope::Everyone => Self::Everyone { progress },
        }
    }

    #[must_use]
    pub const fn own_progress(self) -> Option<u32> {
        match self {
            Self::Individual { progress } | Self::Everyone { progress } => Some(progress),
            Self::Shared => None,
        }
    }

    pub fn own_progress_mut(&mut self) -> Option<&mut u32> {
        match self {
            Self::Individual { progress } | Self::Everyone { progress } => Some(progress),
            Self::Shared => None,
        }
    }
}

pub struct PlayerConnection {
    pub logged_in: bool,
    pub channel: UnboundedSender<ServerToClient>,
    pub name: String,
}

#[derive(Default)]
pub struct PlayerSession {
    // Sequences survive respawn, like the client counter.
    pub last_move_seq: u32,
    pub hops: u32,
    pub score: i32,
    pub quest_states: HashMap<QuestId, PlayerQuestState>,
    pub checkpoint: Option<PlayerCheckpoint>,
    pub checkpoint_visits: BTreeMap<CheckpointId, Vec3>,
}

enum PlayerLifecycle {
    Alive(Entity),
    Dead { respawn_remaining_secs: f32 },
    GroupRespawn,
}

pub struct PlayerLife {
    pub pending_move: Option<CMove>,
    pub processed_move_seq: Option<u32>,
    pub(crate) movement_path: Option<PlayerMovementPath>,
    lifecycle: PlayerLifecycle,
    pub power_ups: [PowerUpState; PowerUpKind::COUNT],
    pub stun_timer: f32,
    pub last_shot_time: f32,
    // Missile ammo, collected from `missile_pack` items up to the configured
    // max. Per-life like `held_keys`. No fire cooldown — ammo is the limit.
    pub missiles: u32,
    // Permanent inventory: a key, once collected, stays held. Kept sorted
    // ascending so the encoded `SPlayerStatus` bytes are deterministic and
    // the client can change-detect via a single equality check.
    pub held_keys: Vec<BarrierKindId>,
    pub fall_state: PlayerFallState,
    pub checkpoint_contact: Option<CheckpointId>,
}

impl PlayerLife {
    fn alive(entity: Entity) -> Self {
        Self::with_lifecycle(PlayerLifecycle::Alive(entity))
    }

    fn with_lifecycle(lifecycle: PlayerLifecycle) -> Self {
        Self {
            lifecycle,
            pending_move: None,
            processed_move_seq: None,
            movement_path: None,
            power_ups: [PowerUpState::Inactive; PowerUpKind::COUNT],
            stun_timer: 0.0,
            last_shot_time: f32::NEG_INFINITY,
            missiles: 0,
            held_keys: Vec::new(),
            fall_state: PlayerFallState::default(),
            checkpoint_contact: None,
        }
    }

    fn begin_respawn(&mut self, respawn_secs: f32) {
        *self = Self::with_lifecycle(PlayerLifecycle::Dead {
            respawn_remaining_secs: respawn_secs,
        });
    }
}

pub struct PlayerInfo {
    pub connection: PlayerConnection,
    pub session: PlayerSession,
    pub life: PlayerLife,
}

impl PlayerInfo {
    #[must_use]
    pub fn new(entity: Entity, channel: UnboundedSender<ServerToClient>) -> Self {
        Self {
            connection: PlayerConnection {
                logged_in: false,
                channel,
                name: String::new(),
            },
            session: PlayerSession::default(),
            life: PlayerLife::alive(entity),
        }
    }

    #[must_use]
    pub fn is_dead(&self) -> bool {
        !matches!(self.life.lifecycle, PlayerLifecycle::Alive(_))
    }

    #[must_use]
    pub fn entity(&self) -> Option<Entity> {
        match self.life.lifecycle {
            PlayerLifecycle::Alive(entity) => Some(entity),
            PlayerLifecycle::Dead { .. } | PlayerLifecycle::GroupRespawn => None,
        }
    }

    // `PlayerMap::begin_respawn` is the only production entry point: it also
    // captures the reset bookkeeping this one skips.
    #[cfg(test)]
    pub fn begin_respawn(&mut self, respawn_secs: f32) {
        self.life.begin_respawn(respawn_secs);
    }

    pub(crate) fn begin_group_respawn(&mut self) {
        self.life = PlayerLife::with_lifecycle(PlayerLifecycle::GroupRespawn);
    }

    #[must_use]
    pub fn respawn_remaining_secs(&self) -> Option<f32> {
        match self.life.lifecycle {
            PlayerLifecycle::Alive(_) | PlayerLifecycle::GroupRespawn => None,
            PlayerLifecycle::Dead {
                respawn_remaining_secs, ..
            } => Some(respawn_remaining_secs),
        }
    }

    pub(crate) fn wait_for_spawn(&mut self) {
        // Waiting for a clear initial spawn is not a death or a world-reset event.
        self.life = PlayerLife::with_lifecycle(PlayerLifecycle::Dead {
            respawn_remaining_secs: 0.0,
        });
    }

    pub fn finish_respawn(&mut self, entity: Entity) {
        self.life.lifecycle = PlayerLifecycle::Alive(entity);
    }

    #[must_use]
    pub fn is_stunned(&self) -> bool {
        self.life.stun_timer > 0.0
    }

    #[must_use]
    pub fn has_key(&self, kind: BarrierKindId) -> bool {
        self.life.held_keys.binary_search(&kind).is_ok()
    }

    // Insert the kind into `held_keys`, keeping it sorted; returns `true` if
    // the kind was newly added (so the caller can decide whether to broadcast
    // an `SPlayerStatus` change), `false` if it was already held.
    pub fn add_key(&mut self, kind: BarrierKindId) -> bool {
        match self.life.held_keys.binary_search(&kind) {
            Ok(_) => false,
            Err(pos) => {
                self.life.held_keys.insert(pos, kind);
                true
            }
        }
    }

    #[must_use]
    pub fn has(&self, kind: PowerUpKind) -> bool {
        self.life.power_ups[kind.index()].is_active()
    }

    #[must_use]
    pub fn has_permanent(&self, kind: PowerUpKind) -> bool {
        self.life.power_ups[kind.index()] == PowerUpState::Permanent
    }

    pub fn erase_equipment(&mut self) -> bool {
        let changed = self.life.missiles > 0 || self.life.power_ups.iter().any(|state| state.is_active());
        self.life.power_ups.fill(PowerUpState::Inactive);
        self.life.missiles = 0;
        changed
    }

    #[must_use]
    pub fn has_speed(&self) -> bool {
        self.has(PowerUpKind::Speed)
    }

    #[must_use]
    pub fn has_low_gravity(&self) -> bool {
        self.has(PowerUpKind::LowGravity)
    }

    // Build the `[bool; N]` array each tick from per-kind `has()` predicates.
    // Used by `status()` (one-shot edge cue) and `snapshot_player()` (durable
    // state).
    pub(crate) fn active_power_ups(&self) -> [bool; PowerUpKind::COUNT] {
        let mut out = [false; PowerUpKind::COUNT];
        for kind in PowerUpKind::ALL {
            out[kind.index()] = self.has(kind);
        }
        out
    }

    pub fn grant_power_up(&mut self, item_type: ItemType, durations: &PowerUpsConfig) {
        let Some(kind) = PowerUpKind::from_item_type(item_type) else {
            unreachable!("non-power-up item passed to grant_power_up");
        };
        self.life.power_ups[kind.index()] = PowerUpState::from_duration(durations.duration_secs_for(kind));
    }

    pub fn try_start_shot(&mut self, now: f32, cooldown_secs: f32, multi_shot: bool) -> bool {
        let kind = if multi_shot {
            PowerUpKind::MultiShot
        } else {
            PowerUpKind::SingleShot
        };
        self.has(kind) && self.try_start_weapon_fire(now, cooldown_secs)
    }

    pub fn try_start_portal_shot(&mut self, now: f32, cooldown_secs: f32) -> bool {
        self.has(PowerUpKind::PortalGun) && self.try_start_weapon_fire(now, cooldown_secs)
    }

    fn try_start_weapon_fire(&mut self, now: f32, cooldown_secs: f32) -> bool {
        if now - self.life.last_shot_time < cooldown_secs {
            return false;
        }
        self.life.last_shot_time = now;
        true
    }

    pub fn try_start_missile(&mut self) -> bool {
        if self.life.missiles == 0 {
            return false;
        }
        self.life.missiles -= 1;
        true
    }

    // Returns the post-add count.
    pub fn add_missiles(&mut self, count: u32, max: u32) -> u32 {
        self.life.missiles = self.life.missiles.saturating_add(count).min(max);
        self.life.missiles
    }

    #[must_use]
    pub fn status(&self, id: PlayerId) -> SPlayerStatus {
        SPlayerStatus {
            id,
            collected: None,
            power_ups: self.active_power_ups(),
            stunned: self.is_stunned(),
            held_keys: self.life.held_keys.clone(),
            missiles: self.life.missiles,
        }
    }

    #[must_use]
    pub fn snapshot_player(
        &self,
        movement: PlayerMovementState,
        health: Health,
        portal_access: PortalAccess,
    ) -> Player {
        Player {
            name: self.connection.name.clone(),
            movement,
            health,
            score: self.session.score,
            power_ups: self.active_power_ups(),
            stunned: self.is_stunned(),
            held_keys: self.life.held_keys.clone(),
            missiles: self.life.missiles,
            portal_access,
            hops: self.session.hops,
        }
    }

    pub fn tick_timers(&mut self, delta: f32) {
        for state in &mut self.life.power_ups {
            state.tick(delta);
        }
        tick_timer(&mut self.life.stun_timer, delta);
    }
}

fn tick_timer(timer: &mut f32, delta: f32) {
    *timer = (*timer - delta).max(0.0);
}

#[derive(Resource, Default)]
pub struct PlayerMap {
    entries: HashMap<PlayerId, PlayerInfo>,
    respawn: RespawnConfig,
    group_respawn: Option<f32>,
    actor_reset_timers: Vec<f32>,
    resets: Vec<PlayerResetCounts>,
    pub(crate) shared_checkpoint: Option<PlayerCheckpoint>,
}

pub(crate) struct PlayerResetCounts {
    pub logged_in: usize,
    pub alive: usize,
}

impl PlayerMap {
    pub fn new(respawn: RespawnConfig) -> Self {
        Self { respawn, ..default() }
    }

    pub(crate) fn begin_respawn(&mut self, id: PlayerId, respawn_secs: f32) -> bool {
        let player_count = self.values().filter(|info| info.connection.logged_in).count();
        let alive = match self.respawn.players {
            PlayerRespawnMode::Individual => self
                .values()
                .filter(|info| info.connection.logged_in && !info.is_dead())
                .count()
                .saturating_sub(1),
            PlayerRespawnMode::Group => 0,
        };
        let Some(info) = self.entries.get_mut(&id).filter(|info| !info.is_dead()) else {
            return false;
        };
        let reset_delay = match self.respawn.players {
            PlayerRespawnMode::Individual => {
                info.life.begin_respawn(respawn_secs);
                respawn_secs
            }
            PlayerRespawnMode::Group => {
                info.begin_group_respawn();
                *self.group_respawn.get_or_insert(respawn_secs)
            }
        };
        if info.connection.logged_in {
            self.record_reset(
                PlayerResetCounts {
                    logged_in: player_count,
                    alive,
                },
                reset_delay,
            );
        }
        true
    }

    fn record_reset(&mut self, counts: PlayerResetCounts, delay: f32) {
        // Capture eligibility now; the countdown must survive departures and later logins.
        if self
            .respawn
            .actors
            .on_player_death
            .applies(counts.logged_in, counts.alive)
            && !self.actor_reset_timers.contains(&delay)
        {
            self.actor_reset_timers.push(delay);
        }
        self.resets.push(counts);
    }

    pub(crate) fn take_resets(&mut self) -> Vec<PlayerResetCounts> {
        std::mem::take(&mut self.resets)
    }

    pub(crate) fn group_respawn_active(&self) -> bool {
        self.group_respawn.is_some()
    }

    pub(crate) fn tick_respawns(&mut self, delta: f32) -> (Vec<PlayerId>, Option<ActorRespawnScope>) {
        let mut to_respawn = Vec::new();
        let mut reset_actors = false;
        self.actor_reset_timers.retain_mut(|remaining| {
            *remaining -= delta;
            if *remaining <= 0.0 {
                reset_actors = true;
                false
            } else {
                true
            }
        });
        if let Some(group) = &mut self.group_respawn {
            *group -= delta;
            if *group <= 0.0 {
                self.group_respawn = None;
                to_respawn.extend(
                    self.iter_mut()
                        .filter(|(_, info)| info.connection.logged_in && info.is_dead())
                        .map(|(id, info)| {
                            // A blocked checkpoint keeps retrying after the shared countdown has ended.
                            info.life.lifecycle = PlayerLifecycle::Dead {
                                respawn_remaining_secs: 0.0,
                            };
                            *id
                        }),
                );
            }
        } else {
            for (id, info) in self.iter_mut() {
                let PlayerLifecycle::Dead { respawn_remaining_secs } = &mut info.life.lifecycle else {
                    continue;
                };
                *respawn_remaining_secs -= delta;
                if *respawn_remaining_secs <= 0.0 {
                    to_respawn.push(*id);
                }
            }
        }
        (to_respawn, reset_actors.then_some(self.respawn.actors.scope))
    }

    pub fn insert(&mut self, id: PlayerId, info: PlayerInfo) -> Option<PlayerInfo> {
        self.entries.insert(id, info)
    }

    pub fn disconnect(&mut self, id: &PlayerId, respawn_secs: f32) -> Option<PlayerInfo> {
        let logged_in = self.values().filter(|info| info.connection.logged_in).count();
        let info = self.entries.remove(id)?;
        if info.connection.logged_in {
            let alive = if self.group_respawn_active() {
                0
            } else {
                self.values()
                    .filter(|info| info.connection.logged_in && !info.is_dead())
                    .count()
            };
            let delay = self
                .group_respawn
                .or(info.respawn_remaining_secs())
                .unwrap_or(respawn_secs);
            self.record_reset(PlayerResetCounts { logged_in, alive }, delay);
        }
        if !self.values().any(|player| player.connection.logged_in) {
            self.shared_checkpoint = None;
        }
        Some(info)
    }

    // "Alex#7" for logs; "player#7" before a name is known.
    #[must_use]
    pub fn describe(&self, id: &PlayerId) -> String {
        match self.get(id) {
            Some(info) if !info.connection.name.is_empty() => format!("{}#{}", info.connection.name, id.0),
            _ => format!("player#{}", id.0),
        }
    }

    // Player-facing name for feed lines; "Player 7" when none is known.
    #[must_use]
    pub fn display_name(&self, id: &PlayerId) -> String {
        match self.get(id) {
            Some(info) if !info.connection.name.is_empty() => info.connection.name.clone(),
            _ => format!("Player {}", id.0),
        }
    }

    #[must_use]
    pub fn get(&self, id: &PlayerId) -> Option<&PlayerInfo> {
        self.entries.get(id)
    }

    pub fn get_mut(&mut self, id: &PlayerId) -> Option<&mut PlayerInfo> {
        self.entries.get_mut(id)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&PlayerId, &PlayerInfo)> {
        self.entries.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&PlayerId, &mut PlayerInfo)> {
        self.entries.iter_mut()
    }

    pub fn values(&self) -> impl Iterator<Item = &PlayerInfo> {
        self.entries.values()
    }

    #[must_use]
    pub fn has_active_players(&self) -> bool {
        self.entries.values().any(|info| info.connection.logged_in)
    }
}
