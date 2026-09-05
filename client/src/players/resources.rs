use bevy::prelude::*;
use std::collections::HashMap;

use common::protocol::{BarrierKindId, Player, PlayerId, Position, PowerUpKind, SPlayerStatus};

use crate::constants::COMMITTED_POSITION_RING_LEN;

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
    // Missile ammo, mirrored from the snapshot (`SMissilesCollected` and the
    // local fire prediction update it early; the snapshot self-heals).
    pub missiles: u32,
    // Recon snap-threshold high-water-mark; updated each tick in
    // `plan_player_moves`. See `RECON_PLAYER_SNAP_DECAY_SECS`.
    pub snap_speed: f32,
    // When the last locally simulated portal crossing happened;
    // reconciliation stands down briefly after it.
    pub last_teleport_time: f32,
}

impl PlayerInfo {
    #[must_use]
    pub fn from_snapshot(entity: Entity, player: &Player) -> Self {
        let mut info = Self {
            entity,
            score: 0,
            name: String::new(),
            power_ups: [false; PowerUpKind::COUNT],
            stunned: false,
            held_keys: Vec::new(),
            missiles: 0,
            snap_speed: 0.0,
            last_teleport_time: f32::NEG_INFINITY,
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
    // "Marc#7" for logs; "player#7" before a name is known.
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

    pub fn retain(&mut self, f: impl FnMut(&PlayerId, &mut PlayerInfo) -> bool) {
        self.0.retain(f);
    }
}

// A ring of the local player's own predicted positions, one per `CMove`,
// keyed by its `seq`: the newest `COMMITTED_POSITION_RING_LEN` are kept, each
// slot overwritten by the sequence that lands on it a ring later. The
// server's `PlayerMove.move_seq` names the slot its position is measured
// against.
pub struct CommittedPositionRing([Option<(u32, Position)>; COMMITTED_POSITION_RING_LEN]);

impl CommittedPositionRing {
    pub fn record(&mut self, seq: u32, pos: Position) {
        self.0[Self::slot(seq)] = Some((seq, pos));
    }

    // `None` unless `seq` is recorded and no later sequence has taken its slot.
    #[must_use]
    pub fn get(&self, seq: u32) -> Option<Position> {
        self.0[Self::slot(seq)].and_then(|(recorded, pos)| (recorded == seq).then_some(pos))
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
            power_ups: [true, true, false],
            stunned: true,
            held_keys: vec![BarrierKindId(1), BarrierKindId(3)],
            missiles: 2,
            portal_access: PortalAccess::None,
        }
    }

    #[test]
    fn from_snapshot_copies_player_info_state() {
        let player = snapshot_player();

        let info = PlayerInfo::from_snapshot(Entity::PLACEHOLDER, &player);

        assert_eq!(info.entity, Entity::PLACEHOLDER);
        assert_eq!(info.score, player.score);
        assert_eq!(info.name, player.name);
        assert_eq!(info.power_ups, player.power_ups);
        assert_eq!(info.stunned, player.stunned);
        assert_eq!(info.held_keys, player.held_keys);
        assert_eq!(info.missiles, player.missiles);
    }

    #[test]
    fn apply_status_updates_status_fields_only() {
        let player = snapshot_player();
        let mut info = PlayerInfo::from_snapshot(Entity::PLACEHOLDER, &player);
        let status = SPlayerStatus {
            id: PlayerId(12),
            power_ups: [false, false, true],
            stunned: false,
            held_keys: vec![BarrierKindId(2)],
        };

        info.apply_status(&status);

        assert_eq!(info.score, player.score);
        assert_eq!(info.name, player.name);
        assert_eq!(info.power_ups, status.power_ups);
        assert_eq!(info.stunned, status.stunned);
        assert_eq!(info.held_keys, status.held_keys);
    }

    #[test]
    fn committed_position_is_found_by_its_seq() {
        let mut positions = CommittedPositionRing::default();
        let pos = Position { x: 1.0, y: 2.0, z: 3.0 };
        positions.record(7, pos);
        assert_eq!(positions.get(7), Some(pos));
    }

    #[test]
    fn committed_position_misses_an_unrecorded_seq() {
        let mut positions = CommittedPositionRing::default();
        assert_eq!(positions.get(0), None);
        positions.record(7, Position::default());
        assert_eq!(positions.get(7 + COMMITTED_POSITION_RING_LEN as u32), None);
    }

    #[test]
    fn committed_position_is_overwritten_a_ring_later() {
        let mut positions = CommittedPositionRing::default();
        positions.record(7, Position::default());
        positions.record(7 + COMMITTED_POSITION_RING_LEN as u32, Position::default());
        assert_eq!(positions.get(7), None);
    }
}
