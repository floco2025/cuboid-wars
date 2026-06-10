use std::collections::HashMap;

use bevy::prelude::*;
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    config::{PowerUpsConfig, Quest, QuestKind},
    net::ServerToClient,
};
use common::{
    constants::{ALWAYS_ANTI_GRAVITY, ALWAYS_MULTI_SHOT, ALWAYS_PHASING, ALWAYS_SPEED, PROJECTILE_COOLDOWN_TIME},
    protocol::{
        BarrierKindId, Health, ItemType, Player, PlayerId, PlayerMoveIntent, PlayerMovementState, Position,
        PowerUpKind, QuestId, SPlayerStatus, SQuestCompleted,
    },
};

// Per-player progress against a single assigned quest. `completed` is
// monotonic — once true it stays true for the rest of the session.
#[derive(Debug, Clone)]
pub struct QuestState {
    pub progress: u32,
    pub completed: bool,
}

pub struct PlayerInfo {
    pub entity: Entity,
    pub logged_in: bool,
    pub channel: UnboundedSender<ServerToClient>,
    pub score: i32,
    pub name: String,
    // Per-kind countdown to power-up expiry. Indexed by `PowerUpKind::index()`.
    // `> 0.0` means active; ticked down by `tick_timers`.
    pub power_up_timers: [f32; PowerUpKind::COUNT],
    pub stun_timer: f32,
    pub last_shot_time: f32,
    // Permanent inventory: a key, once collected, stays held. Kept sorted
    // ascending so the encoded `SPlayerStatus` bytes are deterministic and
    // the client can change-detect via a single equality check.
    pub held_keys: Vec<BarrierKindId>,
    // `Some(remaining_secs)` from the moment a player's health drops to zero
    // until the respawn system spawns a new entity. While dead, the player
    // has no entity and is absent from `SSnapshot` — the local client sees
    // this as "I disappeared from the snapshot" and shows the death overlay.
    pub death_timer: Option<f32>,
    // Per-quest progress, keyed by quest id. Populated at login from the
    // server's quest catalog; persists for the whole session (not cleared
    // by `clear_per_life_state`).
    pub quest_states: HashMap<QuestId, QuestState>,
    // Peak |downward velocity| observed during the current uninterrupted
    // fall, in m/s. Reset to 0 on landing (after the impact damage check)
    // and on respawn.
    pub peak_fall_speed: f32,
}

impl PlayerInfo {
    #[must_use]
    pub fn new(entity: Entity, channel: UnboundedSender<ServerToClient>) -> Self {
        Self {
            entity,
            logged_in: false,
            channel,
            score: 0,
            name: String::new(),
            power_up_timers: [0.0; PowerUpKind::COUNT],
            stun_timer: 0.0,
            last_shot_time: f32::NEG_INFINITY,
            held_keys: Vec::new(),
            death_timer: None,
            quest_states: HashMap::new(),
            peak_fall_speed: 0.0,
        }
    }

    #[must_use]
    pub fn is_dead(&self) -> bool {
        self.death_timer.is_some()
    }

    #[must_use]
    pub fn is_stunned(&self) -> bool {
        self.stun_timer > 0.0
    }

    // Clear per-life state that should not persist across a death: power-ups,
    // stun, keys, and shot cooldown. Score and `quest_states` are
    // intentionally preserved — quests are session-scoped, not per-life.
    pub fn clear_per_life_state(&mut self) {
        self.power_up_timers = [0.0; PowerUpKind::COUNT];
        self.stun_timer = 0.0;
        self.held_keys.clear();
        // Otherwise a player killed with a hot cooldown respawns and can
        // fire before their cooldown would otherwise have ticked down.
        self.last_shot_time = f32::NEG_INFINITY;
        // A respawning player shouldn't inherit the dying player's fall
        // momentum — they'd take damage on their first landing.
        self.peak_fall_speed = 0.0;
    }

    #[must_use]
    pub fn has_key(&self, kind: BarrierKindId) -> bool {
        self.held_keys.binary_search(&kind).is_ok()
    }

    #[must_use]
    pub fn held_keys(&self) -> &[BarrierKindId] {
        &self.held_keys
    }

    // Insert the kind into `held_keys`, keeping it sorted; returns `true` if
    // the kind was newly added (so the caller can decide whether to broadcast
    // an `SPlayerStatus` change), `false` if it was already held.
    pub fn add_key(&mut self, kind: BarrierKindId) -> bool {
        match self.held_keys.binary_search(&kind) {
            Ok(_) => false,
            Err(pos) => {
                self.held_keys.insert(pos, kind);
                true
            }
        }
    }

    #[must_use]
    pub fn has(&self, kind: PowerUpKind) -> bool {
        always_on(kind) || self.power_up_timers[kind.index()] > 0.0
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
    pub fn has_phasing(&self) -> bool {
        self.has(PowerUpKind::Phasing)
    }

    #[must_use]
    pub fn has_anti_gravity(&self) -> bool {
        self.has(PowerUpKind::AntiGravity)
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
        self.power_up_timers[kind.index()] = durations.duration_secs(kind);
    }

    pub fn try_start_shot(&mut self, now: f32) -> Option<bool> {
        if now - self.last_shot_time < PROJECTILE_COOLDOWN_TIME {
            return None;
        }
        self.last_shot_time = now;
        Some(self.has_multi_shot())
    }

    #[must_use]
    pub fn status(&self, id: PlayerId) -> SPlayerStatus {
        SPlayerStatus {
            id,
            power_ups: self.active_power_ups(),
            stunned: self.is_stunned(),
            held_keys: self.held_keys.clone(),
        }
    }

    #[must_use]
    pub fn snapshot_player(
        &self,
        pos: Position,
        move_intent: PlayerMoveIntent,
        face_dir: f32,
        health: Health,
        vertical_velocity: f32,
    ) -> Player {
        Player {
            name: self.name.clone(),
            movement: PlayerMovementState::new(pos, move_intent, vertical_velocity),
            face_dir,
            health,
            score: self.score,
            power_ups: self.active_power_ups(),
            stunned: self.is_stunned(),
            held_keys: self.held_keys.clone(),
        }
    }

    pub fn tick_timers(&mut self, delta: f32) {
        for t in &mut self.power_up_timers {
            tick_timer(t, delta);
        }
        tick_timer(&mut self.stun_timer, delta);
    }
}

// Per-kind debug toggle: when true, the predicate is always on regardless
// of the timer state. Used for quick-test-without-pickup. Wraps the
// pre-existing `ALWAYS_*` constants for the new enum.
#[must_use]
fn always_on(kind: PowerUpKind) -> bool {
    match kind {
        PowerUpKind::Speed => ALWAYS_SPEED,
        PowerUpKind::MultiShot => ALWAYS_MULTI_SHOT,
        PowerUpKind::Phasing => ALWAYS_PHASING,
        PowerUpKind::AntiGravity => ALWAYS_ANTI_GRAVITY,
    }
}

fn tick_timer(timer: &mut f32, delta: f32) {
    *timer = (*timer - delta).max(0.0);
}

// Apply a single cookie pickup to every cookie-kind quest the player has.
// Returns one `SQuestCompleted` per quest that completed *on this call*
// (i.e. the threshold crossing tick) — already-completed quests are
// silently skipped so a player can't trigger the completion twice by
// collecting more cookies.
pub fn record_cookie_for_quests(player_info: &mut PlayerInfo, quests: &[Quest]) -> Vec<SQuestCompleted> {
    let mut completions = Vec::new();
    for quest in quests {
        if !matches!(quest.kind, QuestKind::Cookies) {
            continue;
        }
        let Some(state) = player_info.quest_states.get_mut(&quest.id) else {
            continue;
        };
        if state.completed {
            continue;
        }
        state.progress = state.progress.saturating_add(1);
        if state.progress >= quest.threshold {
            state.completed = true;
            completions.push(SQuestCompleted {
                id: quest.id.clone(),
                completed_text: quest.completed_text.clone(),
            });
        }
    }
    completions
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
    pub fn all_logged_out(&self) -> bool {
        self.0.values().all(|info| !info.logged_in)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bincode::config::standard;
    use tokio::sync::mpsc::unbounded_channel;

    fn dummy_info() -> PlayerInfo {
        // Real channel + real Entity; we only exercise the held_keys path.
        let (tx, _rx) = unbounded_channel();
        PlayerInfo::new(Entity::PLACEHOLDER, tx)
    }

    fn test_power_ups_config() -> PowerUpsConfig {
        PowerUpsConfig {
            max_number: 0,
            despawn_secs: 60.0,
            speed_duration_secs: 1.0,
            multi_shot_duration_secs: 1.0,
            phasing_duration_secs: 1.0,
            anti_gravity_duration_secs: 1.0,
            health_potion_heal_percent: 0.25,
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
            info.held_keys,
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
        info.grant_power_up(ItemType::PhasingPowerUp, &durations);
        info.grant_power_up(ItemType::AntiGravityPowerUp, &durations);

        let status = info.status(PlayerId(7));
        assert!(status.power_up(PowerUpKind::Speed));
        assert!(status.power_up(PowerUpKind::MultiShot));
        assert!(status.power_up(PowerUpKind::Phasing));
        assert!(status.power_up(PowerUpKind::AntiGravity));
    }

    #[test]
    fn try_start_shot_tracks_cooldown_and_multi_shot_state() {
        let mut info = dummy_info();
        let start = 10.0;

        assert_eq!(info.try_start_shot(start), Some(false));
        assert_eq!(info.try_start_shot(start + PROJECTILE_COOLDOWN_TIME * 0.5), None);

        info.grant_power_up(ItemType::MultiShotPowerUp, &test_power_ups_config());
        assert_eq!(
            info.try_start_shot(start + PROJECTILE_COOLDOWN_TIME + f32::EPSILON),
            Some(true)
        );
    }

    #[test]
    fn snapshot_player_uses_same_status_fields_as_status_message() {
        let mut info = dummy_info();
        info.name = "Alice".to_owned();
        info.score = 5;
        info.power_up_timers[PowerUpKind::Speed.index()] = 1.0;
        info.power_up_timers[PowerUpKind::AntiGravity.index()] = 2.0;
        info.stun_timer = 0.5;
        info.add_key(BarrierKindId(1));
        info.add_key(BarrierKindId(3));
        let id = PlayerId(7);
        let pos = Position { x: 1.0, y: 2.0, z: 3.0 };
        let move_intent = PlayerMoveIntent::Running { direction: 0.25 };
        let face_dir = 1.5;
        let health = Health(42.0);
        let vertical_velocity = -3.0;

        let status = info.status(id);
        let player = info.snapshot_player(pos, move_intent, face_dir, health, vertical_velocity);

        assert_eq!(player.name, info.name);
        assert_eq!(player.score, info.score);
        assert_eq!(player.movement.pos, pos);
        assert_eq!(player.movement.move_intent, move_intent);
        assert_eq!(player.movement.vertical_velocity, vertical_velocity);
        assert_eq!(player.face_dir, face_dir);
        assert_eq!(player.health, health);
        assert_eq!(player.power_ups, status.power_ups);
        assert_eq!(player.stunned, status.stunned);
        assert_eq!(player.held_keys, status.held_keys);
    }

    fn cookies_quest(id: &str, threshold: u32) -> Quest {
        Quest {
            id: QuestId(id.to_owned()),
            kind: QuestKind::Cookies,
            threshold,
            announcement_text: "go".to_owned(),
            completed_text: "done".to_owned(),
        }
    }

    #[test]
    fn record_cookie_for_quests_increments_progress() {
        let quest = cookies_quest("collect_gold", 3);
        let mut info = dummy_info();
        info.quest_states.insert(
            quest.id.clone(),
            QuestState {
                progress: 0,
                completed: false,
            },
        );

        let completed = record_cookie_for_quests(&mut info, std::slice::from_ref(&quest));

        assert!(
            completed.is_empty(),
            "first cookie should not complete a 3-threshold quest"
        );
        assert_eq!(info.quest_states[&quest.id].progress, 1);
        assert!(!info.quest_states[&quest.id].completed);
    }

    #[test]
    fn record_cookie_for_quests_flips_completed_on_threshold() {
        let quest = cookies_quest("collect_gold", 2);
        let mut info = dummy_info();
        info.quest_states.insert(
            quest.id.clone(),
            QuestState {
                progress: 0,
                completed: false,
            },
        );

        // First cookie: progress=1, not yet complete.
        let first = record_cookie_for_quests(&mut info, std::slice::from_ref(&quest));
        assert!(first.is_empty());

        // Second cookie: crosses threshold and emits one SQuestCompleted.
        let second = record_cookie_for_quests(&mut info, std::slice::from_ref(&quest));
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].id, quest.id);
        assert_eq!(second[0].completed_text, "done");
        assert!(info.quest_states[&quest.id].completed);
    }

    #[test]
    fn record_cookie_for_quests_is_noop_after_completion() {
        let quest = cookies_quest("collect_gold", 1);
        let mut info = dummy_info();
        info.quest_states.insert(
            quest.id.clone(),
            QuestState {
                progress: 1,
                completed: true,
            },
        );

        let completed = record_cookie_for_quests(&mut info, std::slice::from_ref(&quest));

        // No second-win firing, no progress drift past completion.
        assert!(completed.is_empty());
        assert_eq!(info.quest_states[&quest.id].progress, 1);
    }

    #[test]
    fn clear_per_life_state_preserves_quest_states() {
        let quest = cookies_quest("collect_gold", 10);
        let mut info = dummy_info();
        info.quest_states.insert(
            quest.id.clone(),
            QuestState {
                progress: 7,
                completed: false,
            },
        );
        info.power_up_timers[PowerUpKind::Speed.index()] = 5.0;

        info.clear_per_life_state();

        assert_eq!(
            info.power_up_timers[PowerUpKind::Speed.index()],
            0.0,
            "power-up timers reset by clear"
        );
        assert_eq!(
            info.quest_states[&quest.id].progress, 7,
            "quest progress survives death"
        );
        assert!(!info.quest_states[&quest.id].completed);
    }
}
