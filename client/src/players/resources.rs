use bevy::prelude::*;
use std::collections::HashMap;

use common::protocol::{BarrierKindId, Player, PlayerGeneration, PlayerId, PowerUpKind, SPlayerStatus};

use super::LocalMovementReports;

// My player ID assigned by the server.
#[derive(Resource)]
pub struct MyPlayerId(pub PlayerId);

// Player information (client-side).
pub struct PlayerInfo {
    pub generation: PlayerGeneration,
    pub entity: Entity,
    pub score: i32,
    pub name: String,
    // One bool per `PowerUpKind`, indexed by `PowerUpKind::index()`.
    pub power_ups: [bool; PowerUpKind::COUNT],
    pub stunned: bool,
    // Snapshot-owned key inventory; `SPlayerStatus` updates it early for cues.
    // Sorted ascending so HUD icon order is stable and the change-detection
    // diff is a single equality test.
    pub held_keys: Vec<BarrierKindId>,
    // Missile ammo, mirrored from the snapshot (`SPlayerStatus` and the
    // local fire prediction update it early; the snapshot self-heals).
    pub missiles: u32,
    pub last_movement_tick: u32,
    // The spawn snapshot already places this body past any crossings through this tick.
    pub spawn_tick: u32,
}

impl PlayerInfo {
    #[must_use]
    pub fn from_snapshot(entity: Entity, player: &Player, tick: u32) -> Self {
        let mut info = Self {
            generation: player.generation,
            entity,
            score: 0,
            name: String::new(),
            power_ups: [false; PowerUpKind::COUNT],
            stunned: false,
            held_keys: Vec::new(),
            missiles: 0,
            last_movement_tick: tick,
            spawn_tick: tick,
        };
        info.apply_snapshot(player);
        info
    }

    pub fn apply_snapshot(&mut self, player: &Player) {
        self.score = player.score;
        self.name.clone_from(&player.name);
        self.power_ups = player.power_ups;
        self.stunned = player.stunned;
        self.held_keys.clone_from(&player.held_keys);
        self.missiles = player.missiles;
    }

    pub fn apply_status(&mut self, status: &SPlayerStatus) {
        self.power_ups = status.power_ups;
        self.stunned = status.stunned;
        self.held_keys.clone_from(&status.held_keys);
        self.missiles = status.missiles;
    }

    #[must_use]
    pub const fn power_up(&self, kind: PowerUpKind) -> bool {
        self.power_ups[kind.index()]
    }
}

// Map of all players (client-side source of truth).
#[derive(Resource, Default)]
pub struct PlayerMap {
    players: HashMap<PlayerId, PlayerInfo>,
    // Death cues can overtake snapshots that still contain the retired body.
    retired_bodies: HashMap<PlayerId, PlayerGeneration>,
}

impl PlayerMap {
    // "Alex#7" for logs; "player#7" before a name is known.
    #[must_use]
    pub fn describe(&self, id: &PlayerId) -> String {
        match self.get(id) {
            Some(info) if !info.name.is_empty() => format!("{}#{}", info.name, id.0),
            _ => format!("player#{}", id.0),
        }
    }

    pub fn insert(&mut self, id: PlayerId, info: PlayerInfo) -> Option<PlayerInfo> {
        self.players.insert(id, info)
    }

    // The map is the only record of a player; a body that leaves it takes its
    // retired generation with it.
    pub fn remove(&mut self, id: &PlayerId) -> Option<PlayerInfo> {
        self.retired_bodies.remove(id);
        self.players.remove(id)
    }

    #[must_use]
    pub fn contains_key(&self, id: &PlayerId) -> bool {
        self.players.contains_key(id)
    }

    #[must_use]
    pub fn get(&self, id: &PlayerId) -> Option<&PlayerInfo> {
        self.players.get(id)
    }

    pub fn get_mut(&mut self, id: &PlayerId) -> Option<&mut PlayerInfo> {
        self.players.get_mut(id)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&PlayerId, &PlayerInfo)> {
        self.players.iter()
    }

    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut PlayerInfo> {
        self.players.values_mut()
    }

    pub fn accepts_generation(&self, id: PlayerId, generation: PlayerGeneration) -> bool {
        self.accepts_death(id, generation) && self.retired_bodies.get(&id) != Some(&generation)
    }

    pub fn accepts_death(&self, id: PlayerId, generation: PlayerGeneration) -> bool {
        self.get(&id)
            .is_none_or(|info| generation == info.generation || generation.is_newer_than(info.generation))
            && self
                .retired_bodies
                .get(&id)
                .is_none_or(|retired| generation == *retired || generation.is_newer_than(*retired))
    }

    pub fn accepts_body_cue(&self, id: PlayerId, generation: PlayerGeneration) -> bool {
        self.accepts_generation(id, generation) && self.get(&id).is_some_and(|info| info.generation == generation)
    }

    pub fn retire_body(&mut self, id: PlayerId, generation: PlayerGeneration) -> bool {
        if !self.accepts_death(id, generation) {
            return false;
        }
        self.retired_bodies.insert(id, generation);
        true
    }
}

// Client-only local player state (not synced).
#[derive(Resource)]
pub struct LocalPlayerInfo {
    pub last_shot_time: f32,
    pub reports: LocalMovementReports,
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
            reports: LocalMovementReports::default(),
            stored_yaw: 0.0,
            stored_pitch: 0.0,
            is_dead: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::protocol::{Health, PlayerMoveIntent, PlayerMovementState, PortalAccess, Position};

    fn snapshot_player() -> Player {
        Player {
            generation: PlayerGeneration(0),
            name: "Alice".to_owned(),
            movement: PlayerMovementState::new(Position::default(), PlayerMoveIntent::default(), 0.0, 0.0),
            health: Health(100.0),
            score: 7,
            power_ups: [false, true, false, true, false],
            stunned: true,
            held_keys: vec![BarrierKindId(1), BarrierKindId(3)],
            missiles: 2,
            portal_access: PortalAccess::None,
        }
    }

    #[test]
    fn from_snapshot_copies_player_info_state() {
        let player = snapshot_player();

        let info = PlayerInfo::from_snapshot(Entity::PLACEHOLDER, &player, 42);

        assert_eq!(info.entity, Entity::PLACEHOLDER);
        assert_eq!(info.score, player.score);
        assert_eq!(info.name, player.name);
        assert_eq!(info.power_ups, player.power_ups);
        assert_eq!(info.stunned, player.stunned);
        assert_eq!(info.held_keys, player.held_keys);
        assert_eq!(info.missiles, player.missiles);
        assert_eq!(info.last_movement_tick, 42);
    }

    #[test]
    fn death_blocks_delayed_snapshots_and_cues_but_allows_the_next_body() {
        for generation in [PlayerGeneration(0), PlayerGeneration(u32::MAX)] {
            let id = PlayerId(1);
            let mut player = snapshot_player();
            player.generation = generation;
            let mut players = PlayerMap::default();
            players.insert(id, PlayerInfo::from_snapshot(Entity::PLACEHOLDER, &player, 10));
            assert!(players.retire_body(id, generation));
            assert!(!players.accepts_generation(id, generation));
            assert!(players.accepts_death(id, generation));
            assert!(!players.accepts_body_cue(id, generation));
            player.generation = generation.next();
            assert!(players.accepts_generation(id, player.generation));
            players.insert(id, PlayerInfo::from_snapshot(Entity::PLACEHOLDER, &player, 12));
            assert!(players.accepts_body_cue(id, player.generation));
            assert!(!players.retire_body(id, generation));
            assert!(!players.accepts_body_cue(id, generation));
        }
    }

    #[test]
    fn removing_a_player_forgets_its_retired_body() {
        let id = PlayerId(1);
        let player = snapshot_player();
        let mut players = PlayerMap::default();
        players.insert(id, PlayerInfo::from_snapshot(Entity::PLACEHOLDER, &player, 10));
        assert!(players.retire_body(id, player.generation));
        players.remove(&id);
        assert!(players.accepts_generation(id, player.generation));
        assert!(players.retire_body(id, player.generation));
    }

    #[test]
    fn apply_status_updates_status_fields_only() {
        let player = snapshot_player();
        let mut info = PlayerInfo::from_snapshot(Entity::PLACEHOLDER, &player, 0);
        let status = SPlayerStatus {
            id: PlayerId(12),
            generation: PlayerGeneration(0),
            collected: None,
            power_ups: [false, false, false, false, true],
            stunned: false,
            held_keys: vec![BarrierKindId(2)],
            missiles: 0,
        };

        info.apply_status(&status);

        assert_eq!(info.score, player.score);
        assert_eq!(info.name, player.name);
        assert_eq!(info.power_ups, status.power_ups);
        assert_eq!(info.stunned, status.stunned);
        assert_eq!(info.held_keys, status.held_keys);
        assert_eq!(info.missiles, status.missiles);
    }
}
