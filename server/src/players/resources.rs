use std::collections::HashMap;

use bevy::prelude::*;
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    config::{ActorRespawnScope, PlayerRespawnMode, PowerUpsConfig, RespawnConfig},
    network::ServerToClient,
};
use common::protocol::{
    BarrierKindId, FaceYaw, Health, ItemType, Player, PlayerId, PlayerMarker, PlayerMoveIntent, PlayerMovementState,
    PortalAccess, Position, PowerUpKind, QuestId, QuestScope, SPlayerStatus,
};

use super::{PlayerFallState, PowerUpState};

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
    // Newest `CMove.seq` taken in, applied or held for a crossing; an older
    // commit is ignored. Per session, so a respawn does not reset it under a
    // counter that keeps climbing.
    pub last_move_seq: u32,
    // Portal crossings this player has made, per session like the sequence.
    // An input is expressed on the side the client's own simulation is on
    // and applied only once this player has made the same crossings.
    pub hops: u32,
    pub score: i32,
    pub quest_states: HashMap<QuestId, PlayerQuestState>,
}

enum PlayerLifecycle {
    Alive(Entity),
    Dead { respawn_remaining_secs: f32 },
    GroupRespawn,
}

pub struct PlayerLife {
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
}

impl PlayerLife {
    fn alive(entity: Entity) -> Self {
        Self::with_lifecycle(PlayerLifecycle::Alive(entity))
    }

    fn with_lifecycle(lifecycle: PlayerLifecycle) -> Self {
        Self {
            lifecycle,
            power_ups: [PowerUpState::Inactive; PowerUpKind::COUNT],
            stun_timer: 0.0,
            last_shot_time: f32::NEG_INFINITY,
            missiles: 0,
            held_keys: Vec::new(),
            fall_state: PlayerFallState::default(),
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
    // Used by both `status()` (one-shot edge cue) and `snapshot_player()`
    // (durable state).
    fn active_power_ups(&self) -> [bool; PowerUpKind::COUNT] {
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
        pos: Position,
        move_intent: PlayerMoveIntent,
        face_yaw: f32,
        health: Health,
        vertical_velocity: f32,
        portal_access: PortalAccess,
    ) -> Player {
        Player {
            name: self.connection.name.clone(),
            movement: PlayerMovementState::new(pos, move_intent, vertical_velocity, face_yaw),
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
                    self.iter()
                        .filter(|(_, info)| info.connection.logged_in && info.is_dead())
                        .map(|(id, _)| *id),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ActorRespawnConfig, PowerUpDurationSecs};
    use bincode::config::standard;
    use common::config::DeathTrigger;
    use common::protocol::PortalPairId;
    use tokio::sync::mpsc::unbounded_channel;

    fn dummy_info() -> PlayerInfo {
        // Real channel + real Entity; we only exercise the held_keys path.
        let (tx, _rx) = unbounded_channel();
        PlayerInfo::new(Entity::PLACEHOLDER, tx)
    }

    fn test_power_ups_config() -> PowerUpsConfig {
        PowerUpsConfig {
            duration_secs: PowerUpDurationSecs {
                speed: 1.0,
                single_shot: 0.0,
                multi_shot: 1.0,
                low_gravity: 1.0,
                portal_gun: 0.0,
            },
        }
    }

    fn active_info() -> PlayerInfo {
        let mut info = dummy_info();
        info.connection.logged_in = true;
        info
    }

    #[test]
    fn actor_respawn_policies_use_logged_in_counts_and_preserve_the_requested_scope() {
        for mode in [PlayerRespawnMode::Individual, PlayerRespawnMode::Group] {
            for (trigger, expected) in [
                (DeathTrigger::Never, [false, false]),
                (DeathTrigger::Solo, [true, false]),
                (DeathTrigger::Any, [true, true]),
                (DeathTrigger::All, [true, mode == PlayerRespawnMode::Group]),
            ] {
                for scope in [ActorRespawnScope::Dead, ActorRespawnScope::All] {
                    for count in 1..=2 {
                        let mut players = PlayerMap::new(RespawnConfig {
                            players: mode,
                            actors: ActorRespawnConfig {
                                on_player_death: trigger,
                                scope,
                            },
                        });
                        players.insert(PlayerId(0), dummy_info());
                        for id in 1..=count {
                            players.insert(PlayerId(id), active_info());
                        }
                        assert!(players.begin_respawn(PlayerId(1), 2.0));
                        assert!(!players.begin_respawn(PlayerId(1), 2.0));
                        assert_eq!(players.tick_respawns(1.0), (vec![], None));
                        let (ids, reset) = players.tick_respawns(1.0);
                        assert_eq!(ids, [PlayerId(1)]);
                        assert_eq!(reset, expected[count as usize - 1].then_some(scope));
                    }
                }
            }
        }
    }

    #[test]
    fn solo_actor_reset_eligibility_survives_membership_changes_and_counts_dead_players() {
        let config = RespawnConfig {
            players: PlayerRespawnMode::Individual,
            actors: ActorRespawnConfig {
                on_player_death: DeathTrigger::Solo,
                scope: ActorRespawnScope::All,
            },
        };
        let mut solo = PlayerMap::new(config);
        solo.insert(PlayerId(1), active_info());
        solo.begin_respawn(PlayerId(1), 2.0);
        solo.insert(PlayerId(2), active_info());
        assert_eq!(solo.tick_respawns(2.0).1, Some(ActorRespawnScope::All));

        let mut multiplayer = PlayerMap::new(config);
        multiplayer.insert(PlayerId(1), active_info());
        multiplayer.insert(PlayerId(2), active_info());
        multiplayer.begin_respawn(PlayerId(2), 2.0);
        multiplayer.begin_respawn(PlayerId(1), 2.0);
        multiplayer.disconnect(&PlayerId(2), 2.0);
        assert_eq!(multiplayer.tick_respawns(2.0), (vec![PlayerId(1)], None));
    }

    #[test]
    fn all_actor_reset_waits_for_the_last_death_and_keeps_its_eligibility() {
        let mut players = PlayerMap::new(RespawnConfig {
            actors: ActorRespawnConfig {
                on_player_death: DeathTrigger::All,
                scope: ActorRespawnScope::All,
            },
            ..default()
        });
        players.insert(PlayerId(1), active_info());
        players.insert(PlayerId(2), active_info());
        players.begin_respawn(PlayerId(1), 2.0);
        assert_eq!(players.tick_respawns(1.0), (vec![], None));
        players.begin_respawn(PlayerId(2), 2.0);
        assert_eq!(players.tick_respawns(1.0), (vec![PlayerId(1)], None));
        players
            .get_mut(&PlayerId(1))
            .expect("first player missing")
            .finish_respawn(Entity::PLACEHOLDER);
        players.insert(PlayerId(3), active_info());
        assert_eq!(
            players.tick_respawns(1.0),
            (vec![PlayerId(2)], Some(ActorRespawnScope::All))
        );
    }

    #[test]
    fn disconnecting_the_last_survivor_arms_an_all_actor_reset() {
        let mut players = PlayerMap::new(RespawnConfig {
            actors: ActorRespawnConfig {
                on_player_death: DeathTrigger::All,
                scope: ActorRespawnScope::All,
            },
            ..default()
        });
        players.insert(PlayerId(1), active_info());
        players.insert(PlayerId(2), active_info());
        players.begin_respawn(PlayerId(1), 2.0);
        players.disconnect(&PlayerId(2), 2.0);
        assert_eq!(
            players.tick_respawns(2.0),
            (vec![PlayerId(1)], Some(ActorRespawnScope::All))
        );
    }

    #[test]
    fn logout_actor_policies_use_membership_before_departure_and_do_not_respawn_survivors() {
        for mode in [PlayerRespawnMode::Individual, PlayerRespawnMode::Group] {
            for (trigger, expected) in [
                (DeathTrigger::Never, [false, false]),
                (DeathTrigger::Solo, [true, false]),
                (DeathTrigger::Any, [true, true]),
                (DeathTrigger::All, [true, false]),
            ] {
                for scope in [ActorRespawnScope::Dead, ActorRespawnScope::All] {
                    for count in 1..=2 {
                        let mut players = PlayerMap::new(RespawnConfig {
                            players: mode,
                            actors: ActorRespawnConfig {
                                on_player_death: trigger,
                                scope,
                            },
                        });
                        players.insert(PlayerId(0), dummy_info());
                        for id in 1..=count {
                            players.insert(PlayerId(id), active_info());
                        }
                        players.disconnect(&PlayerId(1), 2.0);
                        assert!(players.disconnect(&PlayerId(1), 2.0).is_none());
                        assert!(!players.group_respawn_active());
                        assert!(players.values().all(|info| !info.is_dead()));
                        assert_eq!(players.tick_respawns(1.0), (vec![], None));
                        assert_eq!(
                            players.tick_respawns(1.0),
                            (vec![], expected[count as usize - 1].then_some(scope)),
                            "{mode:?}, {trigger:?}, {scope:?}, {count} players"
                        );
                        assert_eq!(players.tick_respawns(2.0), (vec![], None));
                    }
                }
            }
        }
    }

    #[test]
    fn logout_during_respawn_keeps_the_remaining_actor_countdown_when_the_server_empties() {
        for mode in [PlayerRespawnMode::Individual, PlayerRespawnMode::Group] {
            let mut players = PlayerMap::new(RespawnConfig {
                players: mode,
                actors: ActorRespawnConfig {
                    on_player_death: DeathTrigger::Solo,
                    scope: ActorRespawnScope::All,
                },
            });
            players.insert(PlayerId(1), active_info());
            players.begin_respawn(PlayerId(1), 2.0);
            assert_eq!(players.tick_respawns(1.0), (vec![], None));
            players.disconnect(&PlayerId(1), 2.0);
            assert!(!players.has_active_players());
            assert_eq!(players.tick_respawns(0.5), (vec![], None));
            assert_eq!(players.tick_respawns(0.5), (vec![], Some(ActorRespawnScope::All)));
            assert_eq!(players.tick_respawns(2.0), (vec![], None));
        }
    }

    #[test]
    fn pending_actor_reset_survives_a_logout_that_no_longer_qualifies() {
        let mut players = PlayerMap::new(RespawnConfig {
            actors: ActorRespawnConfig {
                on_player_death: DeathTrigger::Solo,
                scope: ActorRespawnScope::All,
            },
            ..default()
        });
        players.insert(PlayerId(1), active_info());
        players.begin_respawn(PlayerId(1), 2.0);
        players.tick_respawns(1.0);
        players.insert(PlayerId(2), active_info());
        players.disconnect(&PlayerId(1), 2.0);
        assert_eq!(players.tick_respawns(1.0), (vec![], Some(ActorRespawnScope::All)));
        assert_eq!(players.tick_respawns(2.0), (vec![], None));
    }

    #[test]
    fn unlogged_and_unknown_disconnects_do_not_trigger_world_resets() {
        let mut players = PlayerMap::new(RespawnConfig {
            actors: ActorRespawnConfig {
                on_player_death: DeathTrigger::Any,
                scope: ActorRespawnScope::All,
            },
            ..default()
        });
        players.insert(PlayerId(1), active_info());
        players.insert(PlayerId(0), dummy_info());
        players.disconnect(&PlayerId(0), 2.0);
        assert!(players.disconnect(&PlayerId(99), 2.0).is_none());
        assert!(players.take_resets().is_empty());
        assert_eq!(players.tick_respawns(2.0), (vec![], None));
    }

    #[test]
    fn add_key_is_idempotent_and_keeps_sorted() {
        let mut info = dummy_info();
        assert!(info.add_key(BarrierKindId(2)));
        assert!(info.add_key(BarrierKindId(0)));
        assert!(info.add_key(BarrierKindId(1)));
        // Re-adding any already-held kind returns false (no state change).
        assert!(!info.add_key(BarrierKindId(0)));
        assert!(!info.add_key(BarrierKindId(1)));
        assert!(!info.add_key(BarrierKindId(2)));
        assert_eq!(
            info.life.held_keys,
            vec![BarrierKindId(0), BarrierKindId(1), BarrierKindId(2)]
        );
        assert!(info.has_key(BarrierKindId(1)));
        assert!(!info.has_key(BarrierKindId(3)));
    }

    #[test]
    fn held_keys_round_trip_via_sp_player_status() {
        let mut info = dummy_info();
        info.add_key(BarrierKindId(1));
        info.add_key(BarrierKindId(3));
        let status = info.status(PlayerId(7));
        let encoded = bincode::encode_to_vec(&status, standard()).expect("encode");
        let (decoded, _): (SPlayerStatus, _) = bincode::decode_from_slice(&encoded, standard()).expect("decode");
        assert_eq!(decoded.held_keys, vec![BarrierKindId(1), BarrierKindId(3)]);
        assert_eq!(decoded.id, PlayerId(7));
    }

    #[test]
    fn grant_power_up_sets_matching_status_flag() {
        let mut info = dummy_info();
        let durations = test_power_ups_config();

        info.grant_power_up(ItemType::SpeedPowerUp, &durations);
        info.grant_power_up(ItemType::MultiShotPowerUp, &durations);
        info.grant_power_up(ItemType::LowGravityPowerUp, &durations);

        let status = info.status(PlayerId(7));
        assert!(status.power_up(PowerUpKind::Speed));
        assert!(status.power_up(PowerUpKind::MultiShot));
        assert!(status.power_up(PowerUpKind::LowGravity));
    }

    #[test]
    fn projectile_modes_require_their_own_pickups_and_share_a_cooldown() {
        let mut info = dummy_info();
        const COOLDOWN: f32 = 0.1;
        assert!(!info.try_start_shot(10.0, COOLDOWN, false));
        assert!(!info.try_start_shot(10.0, COOLDOWN, true));
        info.grant_power_up(ItemType::MultiShotPowerUp, &test_power_ups_config());
        assert!(!info.try_start_shot(10.0, COOLDOWN, false));
        assert!(info.try_start_shot(10.0, COOLDOWN, true));
        info.grant_power_up(ItemType::SingleShotPowerUp, &test_power_ups_config());
        assert!(!info.try_start_shot(10.05, COOLDOWN, false));
        assert!(info.try_start_shot(10.11, COOLDOWN, false));
        assert!(!info.try_start_shot(10.15, COOLDOWN, true));
        assert!(info.try_start_shot(10.22, COOLDOWN, true));
        info.tick_timers(1.0);
        assert!(!info.try_start_shot(11.0, COOLDOWN, true));
        assert!(info.try_start_shot(11.0, COOLDOWN, false));
        info.erase_equipment();
        assert!(!info.try_start_shot(12.0, COOLDOWN, false));
        assert!(!info.try_start_shot(12.0, COOLDOWN, true));
    }

    #[test]
    fn single_shot_can_expire_or_last_until_death() {
        let mut info = dummy_info();
        let mut config = test_power_ups_config();
        config.duration_secs.single_shot = 2.0;
        info.grant_power_up(ItemType::SingleShotPowerUp, &config);
        assert!(info.try_start_shot(1.0, 0.1, false));
        info.tick_timers(2.0);
        assert!(!info.try_start_shot(3.0, 0.1, false));
        info.grant_power_up(ItemType::SingleShotPowerUp, &test_power_ups_config());
        info.tick_timers(1000.0);
        assert!(info.try_start_shot(1003.0, 0.1, false));
        info.begin_respawn(1.0);
        info.finish_respawn(Entity::PLACEHOLDER);
        assert!(!info.try_start_shot(1004.0, 0.1, false));
    }

    #[test]
    fn missing_or_expired_gun_rejects_portal_fire_without_spending_cooldown() {
        let mut info = dummy_info();
        info.grant_power_up(ItemType::SingleShotPowerUp, &test_power_ups_config());
        assert!(!info.try_start_portal_shot(1.0, 0.1));
        assert!(info.try_start_shot(1.0, 0.1, false));
        let mut config = test_power_ups_config();
        config.duration_secs.portal_gun = 2.0;
        info.grant_power_up(ItemType::PortalGunPowerUp, &config);
        info.tick_timers(1.0);
        assert!(info.try_start_portal_shot(2.0, 0.1));
        info.grant_power_up(ItemType::PortalGunPowerUp, &config);
        info.tick_timers(1.5);
        assert!(info.has(PowerUpKind::PortalGun));
        info.tick_timers(0.5);
        assert!(!info.try_start_portal_shot(3.0, 0.1));
        assert!(info.try_start_shot(3.0, 0.1, false));
    }

    #[test]
    fn erasure_clears_power_ups_and_ammo_but_preserves_keys_and_progress() {
        let mut info = dummy_info();
        info.session.score = 42;
        info.session
            .quest_states
            .insert(QuestId("quest".into()), PlayerQuestState::Individual { progress: 3 });
        info.life.stun_timer = 2.0;
        info.add_key(BarrierKindId(1));
        info.add_missiles(2, 3);
        for kind in PowerUpKind::ALL {
            info.grant_power_up(kind.to_item_type(), &test_power_ups_config());
        }
        assert!(info.erase_equipment());
        assert!(!info.erase_equipment());
        assert!(PowerUpKind::ALL.into_iter().all(|kind| !info.has(kind)));
        assert_eq!(info.life.held_keys, [BarrierKindId(1)]);
        assert_eq!(info.life.missiles, 0);
        assert_eq!(info.life.stun_timer, 2.0);
        assert_eq!(info.session.score, 42);
        assert_eq!(
            info.session.quest_states[&QuestId("quest".into())].own_progress(),
            Some(3)
        );
    }

    #[test]
    fn projectile_and_portal_shots_share_a_cooldown() {
        let mut info = dummy_info();
        info.grant_power_up(ItemType::SingleShotPowerUp, &test_power_ups_config());
        info.grant_power_up(ItemType::PortalGunPowerUp, &test_power_ups_config());
        const COOLDOWN: f32 = 0.1;

        assert!(info.try_start_portal_shot(10.0, COOLDOWN));
        assert!(!info.try_start_shot(10.05, COOLDOWN, false));
        assert!(info.try_start_shot(10.11, COOLDOWN, false));
        assert!(!info.try_start_portal_shot(10.15, COOLDOWN));
        assert!(info.try_start_portal_shot(10.22, COOLDOWN));
    }

    #[test]
    fn add_missiles_caps_at_max_and_reports_the_new_count() {
        let mut info = dummy_info();
        assert_eq!(info.add_missiles(2, 3), 2);
        assert_eq!(info.add_missiles(5, 3), 3, "adds clamp to the cap");
        assert_eq!(info.add_missiles(0, 3), 3, "zero add is a no-op");
    }

    #[test]
    fn try_start_missile_requires_ammo() {
        let mut info = dummy_info();
        assert!(!info.try_start_missile(), "no ammo");

        info.add_missiles(2, 3);
        assert!(info.try_start_missile());
        assert_eq!(info.life.missiles, 1);
        assert!(info.try_start_missile());
        assert_eq!(info.life.missiles, 0);
        assert!(!info.try_start_missile(), "magazine empty");
    }

    #[test]
    fn add_missiles_clamps_at_max() {
        let mut info = dummy_info();
        assert_eq!(info.add_missiles(2, 3), 2);
        assert_eq!(info.add_missiles(5, 3), 3);
        assert_eq!(info.life.missiles, 3);
    }

    #[test]
    fn begin_respawn_zeroes_missiles() {
        let mut info = dummy_info();
        info.add_missiles(3, 3);

        info.begin_respawn(2.0);

        assert_eq!(info.life.missiles, 0);
        assert_eq!(info.entity(), None);
        assert_eq!(info.respawn_remaining_secs(), Some(2.0));

        let entity = Entity::from_bits(42);
        info.finish_respawn(entity);
        assert_eq!(info.entity(), Some(entity));
        assert_eq!(info.respawn_remaining_secs(), None);
    }

    #[test]
    fn finish_respawn_preserves_life_state_changed_while_dead() {
        let mut info = dummy_info();
        info.begin_respawn(2.0);
        info.add_missiles(1, 3);

        info.finish_respawn(Entity::from_bits(42));

        assert_eq!(info.life.missiles, 1);
    }

    #[test]
    fn snapshot_player_uses_same_status_fields_as_status_message() {
        let mut info = dummy_info();
        info.connection.name = "Alice".to_owned();
        info.session.score = 5;
        info.life.power_ups[PowerUpKind::Speed.index()] = PowerUpState::Timed(1.0);
        info.life.power_ups[PowerUpKind::LowGravity.index()] = PowerUpState::Timed(2.0);
        info.life.stun_timer = 0.5;
        info.add_key(BarrierKindId(1));
        info.add_key(BarrierKindId(3));
        info.add_missiles(2, 3);
        let id = PlayerId(7);
        let pos = Position { x: 1.0, y: 2.0, z: 3.0 };
        let move_intent = PlayerMoveIntent::Running { direction: 0.25 };
        let face_yaw = 1.5;
        let health = Health(42.0);
        let vertical_velocity = -3.0;
        let portal_access = PortalAccess::Both { pair: PortalPairId(1) };

        let status = info.status(id);
        let player = info.snapshot_player(pos, move_intent, face_yaw, health, vertical_velocity, portal_access);

        assert_eq!(player.name, info.connection.name);
        assert_eq!(player.score, info.session.score);
        assert_eq!(player.movement.pos, pos);
        assert_eq!(player.movement.move_intent, move_intent);
        assert_eq!(player.movement.vertical_velocity, vertical_velocity);
        assert_eq!(player.movement.face_yaw, face_yaw);
        assert_eq!(player.health, health);
        assert_eq!(player.power_ups, status.power_ups);
        assert_eq!(player.stunned, status.stunned);
        assert_eq!(player.held_keys, status.held_keys);
        assert_eq!(player.missiles, 2);
        assert_eq!(player.portal_access, portal_access);
    }

    #[test]
    fn begin_respawn_preserves_session_state() {
        let quest_id = QuestId("collect_gold".to_owned());
        let mut info = dummy_info();
        info.session
            .quest_states
            .insert(quest_id.clone(), PlayerQuestState::Individual { progress: 7 });
        info.session.score = 42;
        info.life.power_ups[PowerUpKind::Speed.index()] = PowerUpState::Timed(5.0);

        info.begin_respawn(2.0);

        assert_eq!(
            info.life.power_ups[PowerUpKind::Speed.index()],
            PowerUpState::Inactive,
            "power-up timers reset on death"
        );
        assert_eq!(
            info.session.quest_states[&quest_id].own_progress(),
            Some(7),
            "quest progress survives death"
        );
        assert_eq!(info.session.score, 42, "score survives death");
    }
}
