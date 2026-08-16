use std::collections::HashMap;

use bevy::prelude::*;
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    config::{PowerUpsConfig, Quest, QuestKind},
    network::ServerToClient,
};
use common::{
    constants::{ALWAYS_LOW_GRAVITY, ALWAYS_MULTI_SHOT, ALWAYS_PHASING, ALWAYS_SPEED, PROJECTILE_COOLDOWN_TIME},
    protocol::{
        BarrierKindId, Health, ItemType, NewQuest, Player, PlayerId, PlayerMoveIntent, PlayerMovementState, Position,
        PowerUpKind, QuestId, SPlayerStatus, SQuestCompleted, SQuestProgress, SQuestsAssigned, ServerMessage,
    },
};

// Global debug invincibility. Seeded at startup from the config /
// `--invincible` flag; the `/god` admin command owns it at runtime — which
// is why it's a resource and not a config read.
#[derive(Resource)]
pub struct Invincibility(pub bool);

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
    // Missile ammo, collected from `missile_pack` items up to the configured
    // max. Per-life like `held_keys`. No fire cooldown — ammo is the limit.
    pub missiles: u32,
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
    // and on respawn. Not used for damage (see `fall_peak_y`) — kept as the
    // phantom-fall tripwire: velocity can accumulate without displacement
    // when the support probe misses at an edge while the collider holds.
    pub peak_fall_speed: f32,
    // Highest Y reached during the current airborne window (jump apex counts
    // as the fall start). `NEG_INFINITY` = not airborne. Fall damage is the
    // actual drop `fall_peak_y - landing_y`, immune to fabricated velocity.
    pub fall_peak_y: f32,
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
            missiles: 0,
            held_keys: Vec::new(),
            death_timer: None,
            quest_states: HashMap::new(),
            peak_fall_speed: 0.0,
            fall_peak_y: f32::NEG_INFINITY,
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
        self.missiles = 0;
        // A respawning player shouldn't inherit the dying player's fall
        // momentum — they'd take damage on their first landing.
        self.peak_fall_speed = 0.0;
        self.fall_peak_y = f32::NEG_INFINITY;
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
        self.power_up_timers[kind.index()] = durations.duration_secs(kind);
    }

    pub fn try_start_shot(&mut self, now: f32) -> Option<bool> {
        if now - self.last_shot_time < PROJECTILE_COOLDOWN_TIME {
            return None;
        }
        self.last_shot_time = now;
        Some(self.has_multi_shot())
    }

    // Consumes one missile on success.
    pub fn try_start_missile(&mut self) -> bool {
        if self.missiles == 0 {
            return false;
        }
        self.missiles -= 1;
        true
    }

    // Returns the post-add count.
    pub fn add_missiles(&mut self, count: u32, max: u32) -> u32 {
        self.missiles = self.missiles.saturating_add(count).min(max);
        self.missiles
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
            missiles: self.missiles,
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
        PowerUpKind::LowGravity => ALWAYS_LOW_GRAVITY,
    }
}

fn tick_timer(timer: &mut f32, delta: f32) {
    *timer = (*timer - delta).max(0.0);
}

// A player action that may advance a quest. Lets `record_quest_event` match
// the actor kind for `ActorKills` quests (with an optional per-kind filter).
pub enum QuestEvent<'a> {
    CookieCollected,
    ActorKilled { kind: &'a str },
}

impl QuestEvent<'_> {
    // Does this event advance `quest`? Matches the quest kind, and for actor
    // kills honours the optional `actor_kind` filter (`None` = any actor).
    fn matches(&self, quest: &Quest) -> bool {
        match (quest.kind, self) {
            (QuestKind::Cookies, Self::CookieCollected) => true,
            (QuestKind::ActorKills, Self::ActorKilled { kind }) => {
                quest.actor_kind.as_deref().is_none_or(|want| want == *kind)
            }
            _ => false,
        }
    }
}

// Apply one quest-advancing event to every matching, not-yet-completed quest
// the player holds. Returns the messages to unicast: a `QuestProgress` for a
// quest that advanced, or a `QuestCompleted` for one that crossed its
// threshold on this call. Already-completed quests are skipped so a win can't
// fire twice.
pub fn record_quest_event(player_info: &mut PlayerInfo, quests: &[Quest], event: QuestEvent) -> Vec<ServerMessage> {
    let mut messages = Vec::new();
    for quest in quests {
        if !event.matches(quest) {
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
            messages.push(ServerMessage::QuestCompleted(SQuestCompleted {
                id: quest.id.clone(),
                completed_text: quest.completed_text.clone(),
            }));
        } else {
            messages.push(ServerMessage::QuestProgress(SQuestProgress {
                id: quest.id.clone(),
                progress: state.progress,
            }));
        }
    }
    messages
}

// Assign every quest the player doesn't already hold, seeding fresh progress
// state, and return the batch to unicast as one combined announcement (when
// any were newly assigned). This is the single seam for granting quests — at
// login or from a future in-game quest-giver. Re-granting an already-held
// quest is a no-op: no progress reset, no re-announce.
pub fn assign_quests(player_info: &mut PlayerInfo, quests: &[Quest]) -> Option<SQuestsAssigned> {
    let mut new_quests = Vec::new();
    // `order` is the catalog index so display order = `gameplay.json` order.
    // Indexing the passed slice is correct for the login grant (full catalog);
    // a future quest-giver should pass the full catalog too to keep ranks stable.
    for (index, quest) in quests.iter().enumerate() {
        if player_info.quest_states.contains_key(&quest.id) {
            continue;
        }
        player_info.quest_states.insert(
            quest.id.clone(),
            QuestState {
                progress: 0,
                completed: false,
            },
        );
        new_quests.push(NewQuest {
            id: quest.id.clone(),
            title: quest.title.clone(),
            description: quest.description.clone(),
            progress: 0,
            threshold: quest.threshold,
            order: index as u32,
        });
    }
    (!new_quests.is_empty()).then_some(SQuestsAssigned { quests: new_quests })
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
            speed_duration_secs: 1.0,
            multi_shot_duration_secs: 1.0,
            phasing_duration_secs: 1.0,
            low_gravity_duration_secs: 1.0,
            health_potion_heal_fraction: 0.25,
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
        info.grant_power_up(ItemType::LowGravityPowerUp, &durations);

        let status = info.status(PlayerId(7));
        assert!(status.power_up(PowerUpKind::Speed));
        assert!(status.power_up(PowerUpKind::MultiShot));
        assert!(status.power_up(PowerUpKind::Phasing));
        assert!(status.power_up(PowerUpKind::LowGravity));
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
    fn try_start_missile_requires_ammo() {
        let mut info = dummy_info();
        assert!(!info.try_start_missile(), "no ammo");

        info.add_missiles(2, 3);
        assert!(info.try_start_missile());
        assert!(info.try_start_missile());
        assert_eq!(info.missiles, 0);
        assert!(!info.try_start_missile(), "magazine empty");
    }

    #[test]
    fn add_missiles_clamps_at_max() {
        let mut info = dummy_info();
        assert_eq!(info.add_missiles(2, 3), 2);
        assert_eq!(info.add_missiles(5, 3), 3);
        assert_eq!(info.missiles, 3);
    }

    #[test]
    fn clear_per_life_state_zeroes_missiles() {
        let mut info = dummy_info();
        info.add_missiles(3, 3);

        info.clear_per_life_state();

        assert_eq!(info.missiles, 0);
    }

    #[test]
    fn snapshot_player_uses_same_status_fields_as_status_message() {
        let mut info = dummy_info();
        info.name = "Alice".to_owned();
        info.score = 5;
        info.power_up_timers[PowerUpKind::Speed.index()] = 1.0;
        info.power_up_timers[PowerUpKind::LowGravity.index()] = 2.0;
        info.stun_timer = 0.5;
        info.add_key(BarrierKindId(1));
        info.add_key(BarrierKindId(3));
        info.add_missiles(2, 3);
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
        assert_eq!(player.missiles, 2);
    }

    fn cookies_quest(id: &str, threshold: u32) -> Quest {
        Quest {
            id: QuestId(id.to_owned()),
            kind: QuestKind::Cookies,
            actor_kind: None,
            threshold,
            title: "Gold".to_owned(),
            description: "collect gold".to_owned(),
            completed_text: "done".to_owned(),
        }
    }

    fn sentry_quest(id: &str, threshold: u32) -> Quest {
        Quest {
            id: QuestId(id.to_owned()),
            kind: QuestKind::ActorKills,
            actor_kind: Some("sentry".to_owned()),
            threshold,
            title: "Hunt".to_owned(),
            description: "destroy sentries".to_owned(),
            completed_text: "hunted".to_owned(),
        }
    }

    fn seed_quest(info: &mut PlayerInfo, quest: &Quest) {
        info.quest_states.insert(
            quest.id.clone(),
            QuestState {
                progress: 0,
                completed: false,
            },
        );
    }

    #[test]
    fn record_quest_event_increments_progress() {
        let quest = cookies_quest("collect_gold", 3);
        let mut info = dummy_info();
        seed_quest(&mut info, &quest);

        let msgs = record_quest_event(&mut info, std::slice::from_ref(&quest), QuestEvent::CookieCollected);

        assert!(
            matches!(msgs.as_slice(), [ServerMessage::QuestProgress(p)] if p.progress == 1 && p.id == quest.id),
            "first cookie emits one progress update at 1/3"
        );
        assert_eq!(info.quest_states[&quest.id].progress, 1);
        assert!(!info.quest_states[&quest.id].completed);
    }

    #[test]
    fn record_quest_event_flips_completed_on_threshold() {
        let quest = cookies_quest("collect_gold", 2);
        let mut info = dummy_info();
        seed_quest(&mut info, &quest);

        // First cookie: progress=1, not yet complete.
        let first = record_quest_event(&mut info, std::slice::from_ref(&quest), QuestEvent::CookieCollected);
        assert!(matches!(first.as_slice(), [ServerMessage::QuestProgress(_)]));

        // Second cookie: crosses threshold and emits one QuestCompleted.
        let second = record_quest_event(&mut info, std::slice::from_ref(&quest), QuestEvent::CookieCollected);
        assert!(
            matches!(second.as_slice(), [ServerMessage::QuestCompleted(c)] if c.id == quest.id && c.completed_text == "done")
        );
        assert!(info.quest_states[&quest.id].completed);
    }

    #[test]
    fn record_quest_event_is_noop_after_completion() {
        let quest = cookies_quest("collect_gold", 1);
        let mut info = dummy_info();
        info.quest_states.insert(
            quest.id.clone(),
            QuestState {
                progress: 1,
                completed: true,
            },
        );

        let msgs = record_quest_event(&mut info, std::slice::from_ref(&quest), QuestEvent::CookieCollected);

        // No second-win firing, no progress drift past completion.
        assert!(msgs.is_empty());
        assert_eq!(info.quest_states[&quest.id].progress, 1);
    }

    #[test]
    fn record_quest_event_actor_kill_respects_kind_filter() {
        let quest = sentry_quest("destroy_sentries", 2);
        let mut info = dummy_info();
        seed_quest(&mut info, &quest);

        // A mine kill must not advance a sentry-filtered quest.
        let mine = record_quest_event(
            &mut info,
            std::slice::from_ref(&quest),
            QuestEvent::ActorKilled { kind: "zapper" },
        );
        assert!(mine.is_empty());
        assert_eq!(info.quest_states[&quest.id].progress, 0);

        // A sentry kill advances it.
        let sentry = record_quest_event(
            &mut info,
            std::slice::from_ref(&quest),
            QuestEvent::ActorKilled { kind: "sentry" },
        );
        assert!(matches!(sentry.as_slice(), [ServerMessage::QuestProgress(p)] if p.progress == 1));
    }

    #[test]
    fn record_quest_event_ignores_nonmatching_kind() {
        // A cookie event advances only the cookie quest, leaving the actor quest untouched.
        let cookie = cookies_quest("collect_gold", 5);
        let sentry = sentry_quest("destroy_sentries", 5);
        let mut info = dummy_info();
        seed_quest(&mut info, &cookie);
        seed_quest(&mut info, &sentry);
        let quests = [cookie.clone(), sentry.clone()];

        let msgs = record_quest_event(&mut info, &quests, QuestEvent::CookieCollected);

        assert_eq!(msgs.len(), 1, "only the cookie quest advances");
        assert_eq!(info.quest_states[&cookie.id].progress, 1);
        assert_eq!(info.quest_states[&sentry.id].progress, 0);
    }

    #[test]
    fn assign_quests_seeds_state_and_batches_new_quests() {
        let quests = [cookies_quest("collect_gold", 10), sentry_quest("destroy_sentries", 4)];
        let mut info = dummy_info();

        let batch = assign_quests(&mut info, &quests).expect("two new quests assigned");

        assert_eq!(batch.quests.len(), 2);
        assert!(batch.quests.iter().all(|q| q.progress == 0));
        assert!(
            batch.quests.iter().any(|q| q.id == quests[1].id && q.threshold == 4),
            "batch carries each quest's threshold"
        );
        assert_eq!(info.quest_states.len(), 2);
    }

    #[test]
    fn assign_quests_is_idempotent() {
        let quests = [cookies_quest("collect_gold", 10)];
        let mut info = dummy_info();
        assign_quests(&mut info, &quests).expect("first assignment");

        // Advance, then re-assign the same quest: must not reset or re-announce.
        record_quest_event(&mut info, &quests, QuestEvent::CookieCollected);
        let second = assign_quests(&mut info, &quests);

        assert!(second.is_none());
        assert_eq!(info.quest_states[&quests[0].id].progress, 1);
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
