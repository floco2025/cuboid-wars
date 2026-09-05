use std::collections::HashMap;

use bevy::prelude::*;
use tokio::sync::mpsc::UnboundedSender;

use crate::{config::PowerUpsConfig, network::ServerToClient};
use common::protocol::{
    BarrierKindId, FaceYaw, Health, ItemType, Player, PlayerId, PlayerMarker, PlayerMoveIntent, PlayerMovementState,
    PortalAccess, Position, PowerUpKind, QuestId, QuestScope, SPlayerStatus,
};

use super::PlayerFallState;

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

// Global debug invincibility. Seeded at startup from the config /
// `--invincible` flag; the `/god` admin command owns it at runtime — which
// is why it's a resource and not a config read.
#[derive(Resource)]
pub struct Invincibility(pub bool);

// Global unlimited missile ammo. A separate flag from `Invincibility` so the
// two effects stay independently wireable, but `/god` toggles them together.
#[derive(Resource)]
pub struct UnlimitedMissiles(pub bool);

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
}

pub struct PlayerLife {
    lifecycle: PlayerLifecycle,
    // Per-kind countdown to power-up expiry. Indexed by `PowerUpKind::index()`.
    // `> 0.0` means active; ticked down by `tick_timers`.
    pub power_up_timers: [f32; PowerUpKind::COUNT],
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
            power_up_timers: [0.0; PowerUpKind::COUNT],
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
        matches!(self.life.lifecycle, PlayerLifecycle::Dead { .. })
    }

    #[must_use]
    pub fn entity(&self) -> Option<Entity> {
        match self.life.lifecycle {
            PlayerLifecycle::Alive(entity) => Some(entity),
            PlayerLifecycle::Dead { .. } => None,
        }
    }

    pub fn begin_respawn(&mut self, respawn_secs: f32) {
        self.life.begin_respawn(respawn_secs);
    }

    pub fn respawn_remaining_secs_mut(&mut self) -> Option<&mut f32> {
        match &mut self.life.lifecycle {
            PlayerLifecycle::Alive(_) => None,
            PlayerLifecycle::Dead { respawn_remaining_secs } => Some(respawn_remaining_secs),
        }
    }

    #[must_use]
    pub fn respawn_remaining_secs(&self) -> Option<f32> {
        match self.life.lifecycle {
            PlayerLifecycle::Alive(_) => None,
            PlayerLifecycle::Dead { respawn_remaining_secs } => Some(respawn_remaining_secs),
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
        self.life.power_up_timers[kind.index()] > 0.0
    }

    #[must_use]
    pub fn has_speed(&self) -> bool {
        self.has(PowerUpKind::Speed)
    }

    #[must_use]
    pub fn has_multi_shot(&self) -> bool {
        self.has(PowerUpKind::MultiShot)
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
            unreachable!("only timer-based power-ups call grant_power_up; health potion is applied to Health directly");
        };
        self.life.power_up_timers[kind.index()] = durations.duration_secs_for(kind);
    }

    pub fn try_start_shot(&mut self, now: f32, cooldown_secs: f32) -> Option<bool> {
        if !self.try_start_weapon_fire(now, cooldown_secs) {
            return None;
        }
        Some(self.has_multi_shot())
    }

    pub fn try_start_portal_shot(&mut self, now: f32, cooldown_secs: f32) -> bool {
        self.try_start_weapon_fire(now, cooldown_secs)
    }

    fn try_start_weapon_fire(&mut self, now: f32, cooldown_secs: f32) -> bool {
        if now - self.life.last_shot_time < cooldown_secs {
            return false;
        }
        self.life.last_shot_time = now;
        true
    }

    // Consumes one missile on success unless unlimited ammo is active.
    pub fn try_start_missile(&mut self, unlimited: bool) -> bool {
        if unlimited {
            return true;
        }
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
            power_ups: self.active_power_ups(),
            stunned: self.is_stunned(),
            held_keys: self.life.held_keys.clone(),
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
        for t in &mut self.life.power_up_timers {
            tick_timer(t, delta);
        }
        tick_timer(&mut self.life.stun_timer, delta);
    }
}

fn tick_timer(timer: &mut f32, delta: f32) {
    *timer = (*timer - delta).max(0.0);
}

#[derive(Resource, Default)]
pub struct PlayerMap(HashMap<PlayerId, PlayerInfo>);

impl PlayerMap {
    pub fn insert(&mut self, id: PlayerId, info: PlayerInfo) -> Option<PlayerInfo> {
        self.0.insert(id, info)
    }

    pub fn remove(&mut self, id: &PlayerId) -> Option<PlayerInfo> {
        self.0.remove(id)
    }

    // "Marc#7" for logs; "player#7" before a name is known.
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
        self.0.get(id)
    }

    pub fn get_mut(&mut self, id: &PlayerId) -> Option<&mut PlayerInfo> {
        self.0.get_mut(id)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&PlayerId, &PlayerInfo)> {
        self.0.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&PlayerId, &mut PlayerInfo)> {
        self.0.iter_mut()
    }

    pub fn values(&self) -> impl Iterator<Item = &PlayerInfo> {
        self.0.values()
    }

    #[must_use]
    pub fn has_active_players(&self) -> bool {
        self.0.values().any(|info| info.connection.logged_in)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::PowerUpDurationSecs;
    use bincode::config::standard;
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
                multi_shot: 1.0,
                low_gravity: 1.0,
            },
        }
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
    fn try_start_shot_tracks_cooldown_and_multi_shot_state() {
        let mut info = dummy_info();
        let start = 10.0;

        const COOLDOWN: f32 = 0.1;
        assert_eq!(info.try_start_shot(start, COOLDOWN), Some(false));
        assert_eq!(info.try_start_shot(start + COOLDOWN * 0.5, COOLDOWN), None);

        info.grant_power_up(ItemType::MultiShotPowerUp, &test_power_ups_config());
        assert_eq!(
            info.try_start_shot(start + COOLDOWN + f32::EPSILON, COOLDOWN),
            Some(true)
        );
    }

    #[test]
    fn projectile_and_portal_shots_share_a_cooldown() {
        let mut info = dummy_info();
        const COOLDOWN: f32 = 0.1;

        assert!(info.try_start_portal_shot(10.0, COOLDOWN));
        assert_eq!(info.try_start_shot(10.05, COOLDOWN), None);
        assert_eq!(info.try_start_shot(10.11, COOLDOWN), Some(false));
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
        assert!(!info.try_start_missile(false), "no ammo");

        info.add_missiles(2, 3);
        assert!(info.try_start_missile(false));
        assert!(info.try_start_missile(false));
        assert_eq!(info.life.missiles, 0);
        assert!(!info.try_start_missile(false), "magazine empty");
        assert!(info.try_start_missile(true), "unlimited fire ignores the magazine");
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
        info.life.power_up_timers[PowerUpKind::Speed.index()] = 1.0;
        info.life.power_up_timers[PowerUpKind::LowGravity.index()] = 2.0;
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
        info.life.power_up_timers[PowerUpKind::Speed.index()] = 5.0;

        info.begin_respawn(2.0);

        assert_eq!(
            info.life.power_up_timers[PowerUpKind::Speed.index()],
            0.0,
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
