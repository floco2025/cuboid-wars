use bevy::prelude::*;
use std::collections::HashMap;

use common::protocol::{BarrierKindId, Player, PlayerId, Position, PowerUpKind, SPlayerStatus};

use crate::portals::LocalPortalCrossings;

const COMMITTED_POSITION_RING_LEN: usize = 64;

// My player ID assigned by the server.
#[derive(Resource)]
pub struct MyPlayerId(pub PlayerId);

// Player information (client-side).
pub struct PlayerInfo {
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
}

impl PlayerInfo {
    #[must_use]
    pub fn from_snapshot(entity: Entity, player: &Player, tick: u32) -> Self {
        let mut info = Self {
            entity,
            score: 0,
            name: String::new(),
            power_ups: [false; PowerUpKind::COUNT],
            stunned: false,
            held_keys: Vec::new(),
            missiles: 0,
            last_movement_tick: tick,
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
pub struct PlayerMap(HashMap<PlayerId, PlayerInfo>);

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

    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut PlayerInfo> {
        self.0.values_mut()
    }

    pub fn retain(&mut self, f: impl FnMut(&PlayerId, &mut PlayerInfo) -> bool) {
        self.0.retain(f);
    }
}

// Sequences pair the local result with the server comparison; ticks also
// feed clock synchronization. A snap invalidates all recorded positions.
pub struct CommittedPositionRing([Option<CommittedPosition>; COMMITTED_POSITION_RING_LEN]);

#[derive(Clone, Copy)]
pub struct CommittedPosition {
    seq: u32,
    pub tick: u32,
    pub pos: Position,
}

impl CommittedPositionRing {
    pub fn record(&mut self, seq: u32, tick: u32, pos: Position) {
        self.0[Self::slot(seq)] = Some(CommittedPosition { seq, tick, pos });
    }

    #[must_use]
    pub fn get(&self, seq: u32) -> Option<CommittedPosition> {
        self.0[Self::slot(seq)].filter(|c| c.seq == seq)
    }

    #[must_use]
    pub fn tick_for_seq(&self, seq: u32) -> Option<u32> {
        self.get(seq).map(|c| c.tick)
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    fn slot(seq: u32) -> usize {
        seq as usize % COMMITTED_POSITION_RING_LEN
    }
}

impl Default for CommittedPositionRing {
    fn default() -> Self {
        Self([None; COMMITTED_POSITION_RING_LEN])
    }
}

// Client-only local player state (not synced).
#[derive(Resource)]
pub struct LocalPlayerInfo {
    pub last_shot_time: f32,
    // Stamped on every `CMove`; the server ignores an older one.
    pub move_seq: u32,
    pub committed_positions: CommittedPositionRing,
    pub last_comparison_seq: Option<u32>,
    pub portal_crossings: LocalPortalCrossings,
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
            move_seq: 0,
            committed_positions: CommittedPositionRing::default(),
            last_comparison_seq: None,
            portal_crossings: LocalPortalCrossings::default(),
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
    fn apply_status_updates_status_fields_only() {
        let player = snapshot_player();
        let mut info = PlayerInfo::from_snapshot(Entity::PLACEHOLDER, &player, 0);
        let status = SPlayerStatus {
            collected: None,
            id: PlayerId(12),
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

    #[test]
    fn committed_position_is_found_by_its_seq() {
        let mut positions = CommittedPositionRing::default();
        let pos = Position { x: 1.0, y: 2.0, z: 3.0 };
        positions.record(7, 100, pos);
        let recorded = positions.get(7).expect("seq 7 missing from the ring");
        assert_eq!(recorded.pos, pos);
        assert_eq!(recorded.tick, 100);
    }

    #[test]
    fn committed_position_misses_an_unrecorded_seq() {
        let mut positions = CommittedPositionRing::default();
        assert!(positions.get(0).is_none());
        assert!(positions.tick_for_seq(0).is_none());
        positions.record(7, 0, Position::default());
        assert!(positions.get(7 + COMMITTED_POSITION_RING_LEN as u32).is_none());
        assert!(positions.tick_for_seq(7 + COMMITTED_POSITION_RING_LEN as u32).is_none());
    }

    #[test]
    fn committed_position_is_overwritten_a_ring_later() {
        let mut positions = CommittedPositionRing::default();
        positions.record(7, 0, Position::default());
        positions.record(7 + COMMITTED_POSITION_RING_LEN as u32, 0, Position::default());
        assert!(positions.get(7).is_none());
        assert!(positions.tick_for_seq(7).is_none());
    }

    #[test]
    fn cleared_ring_holds_nothing() {
        let mut positions = CommittedPositionRing::default();
        positions.record(7, 0, Position::default());
        positions.clear();
        assert!(positions.get(7).is_none());
        assert!(positions.tick_for_seq(7).is_none());
    }
}
