use bevy::prelude::*;
use std::collections::HashMap;

use common::protocol::{BarrierKindId, Player, PlayerId, PlayerMoveIntent, SPlayerStatus};

// My player ID assigned by the server.
#[derive(Resource)]
pub struct MyPlayerId(pub PlayerId);

// Player information (client-side).
pub struct PlayerInfo {
    pub entity: Entity,
    pub score: i32,
    pub name: String,
    pub speed_power_up: bool,
    pub multi_shot_power_up: bool,
    pub phasing_power_up: bool,
    pub anti_gravity_power_up: bool,
    pub stunned: bool,
    // Snapshot-owned key inventory; `SPlayerStatus` updates it early for cues.
    // Sorted ascending so HUD icon order is stable and the change-detection
    // diff is a single equality test.
    pub held_keys: Vec<BarrierKindId>,
}

impl PlayerInfo {
    #[must_use]
    pub fn from_snapshot(entity: Entity, player: &Player) -> Self {
        let mut info = Self {
            entity,
            score: 0,
            name: String::new(),
            speed_power_up: false,
            multi_shot_power_up: false,
            phasing_power_up: false,
            anti_gravity_power_up: false,
            stunned: false,
            held_keys: Vec::new(),
        };
        info.apply_snapshot(player);
        info
    }

    pub fn apply_snapshot(&mut self, player: &Player) {
        self.score = player.score;
        self.name.clone_from(&player.name);
        self.speed_power_up = player.speed_power_up;
        self.multi_shot_power_up = player.multi_shot_power_up;
        self.phasing_power_up = player.phasing_power_up;
        self.anti_gravity_power_up = player.anti_gravity_power_up;
        self.stunned = player.stunned;
        self.held_keys.clone_from(&player.held_keys);
    }

    pub fn apply_status(&mut self, status: &SPlayerStatus) {
        self.speed_power_up = status.speed_power_up;
        self.multi_shot_power_up = status.multi_shot_power_up;
        self.phasing_power_up = status.phasing_power_up;
        self.anti_gravity_power_up = status.anti_gravity_power_up;
        self.stunned = status.stunned;
        self.held_keys.clone_from(&status.held_keys);
    }
}

// Map of all players (client-side source of truth).
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
    pub fn contains_key(&self, id: &PlayerId) -> bool {
        self.0.contains_key(id)
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

    pub fn retain(&mut self, f: impl FnMut(&PlayerId, &mut PlayerInfo) -> bool) {
        self.0.retain(f);
    }
}

// Client-only local player state (not synced).
#[derive(Resource)]
pub struct LocalPlayerInfo {
    pub last_shot_time: f32,
    pub last_sent_move_intent: PlayerMoveIntent,
    pub last_sent_face: f32,
    pub stored_yaw: f32,
    pub stored_pitch: f32,
    // True from the moment the local player vanishes from `SSnapshot` until
    // they reappear (death → respawn). Input systems gate on this; the death
    // overlay reads it to show/hide the red tint.
    pub is_dead: bool,
}

impl Default for LocalPlayerInfo {
    fn default() -> Self {
        Self {
            last_shot_time: f32::NEG_INFINITY,
            last_sent_move_intent: PlayerMoveIntent::default(),
            last_sent_face: 0.0,
            stored_yaw: 0.0,
            stored_pitch: 0.0,
            is_dead: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::protocol::{Health, PlayerMovementState, Position};

    fn snapshot_player() -> Player {
        Player {
            name: "Alice".to_owned(),
            movement: PlayerMovementState::new(Position::default(), PlayerMoveIntent::default(), 0.0),
            face_dir: 0.0,
            health: Health(100.0),
            score: 7,
            speed_power_up: true,
            multi_shot_power_up: true,
            phasing_power_up: false,
            anti_gravity_power_up: true,
            stunned: true,
            held_keys: vec![BarrierKindId(1), BarrierKindId(3)],
        }
    }

    #[test]
    fn from_snapshot_copies_player_info_state() {
        let player = snapshot_player();

        let info = PlayerInfo::from_snapshot(Entity::PLACEHOLDER, &player);

        assert_eq!(info.entity, Entity::PLACEHOLDER);
        assert_eq!(info.score, player.score);
        assert_eq!(info.name, player.name);
        assert_eq!(info.speed_power_up, player.speed_power_up);
        assert_eq!(info.multi_shot_power_up, player.multi_shot_power_up);
        assert_eq!(info.phasing_power_up, player.phasing_power_up);
        assert_eq!(info.anti_gravity_power_up, player.anti_gravity_power_up);
        assert_eq!(info.stunned, player.stunned);
        assert_eq!(info.held_keys, player.held_keys);
    }

    #[test]
    fn apply_status_updates_status_fields_only() {
        let player = snapshot_player();
        let mut info = PlayerInfo::from_snapshot(Entity::PLACEHOLDER, &player);
        let status = SPlayerStatus {
            id: PlayerId(12),
            speed_power_up: false,
            multi_shot_power_up: false,
            phasing_power_up: true,
            anti_gravity_power_up: false,
            stunned: false,
            held_keys: vec![BarrierKindId(2)],
        };

        info.apply_status(&status);

        assert_eq!(info.score, player.score);
        assert_eq!(info.name, player.name);
        assert_eq!(info.speed_power_up, status.speed_power_up);
        assert_eq!(info.multi_shot_power_up, status.multi_shot_power_up);
        assert_eq!(info.phasing_power_up, status.phasing_power_up);
        assert_eq!(info.anti_gravity_power_up, status.anti_gravity_power_up);
        assert_eq!(info.stunned, status.stunned);
        assert_eq!(info.held_keys, status.held_keys);
    }
}
