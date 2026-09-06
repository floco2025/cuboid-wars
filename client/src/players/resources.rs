use bevy::prelude::*;
use std::collections::HashMap;

use common::protocol::{BarrierKindId, Player, PlayerId, Position, PowerUpKind, SPlayerStatus, sequence_is_newer};

use crate::constants::{COMMITTED_POSITION_RING_LEN, HOP_DISPUTE_SLACK_TICKS};

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
    // Portal crossings this client's own simulation of the player has made,
    // seeded from the snapshot. A server state pairs with that simulation
    // only while its count matches.
    pub hops: u32,
    // The tick our simulation made its last crossing at (the snapshot's tick
    // at appearance). A state from before it is from the other side by
    // right, not in dispute.
    pub hop_tick: u32,
    // The server tick of the first state disputing our count, while a
    // dispute is open.
    pub disputed_since: Option<u32>,
}

// How a server state relates to our simulation of the player: on the same
// side of the same crossings, from the other side (not to be steered or
// reconciled from), or settling a dispute for the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrossingVerdict {
    Paired,
    Skipped,
    Settled,
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
            snap_speed: 0.0,
            hops: player.hops,
            hop_tick: tick,
            disputed_since: None,
        };
        info.apply_snapshot(player);
        info
    }

    // Judges a state carrying `hops` at server tick `tick`. A state from
    // before our own crossing is from the other side by right and is only
    // skipped; one from at or after it without the crossing, or one carrying
    // a crossing we never made, is evidence of a misprediction, and evidence
    // outlasting `HOP_DISPUTE_SLACK_TICKS` settles for the server, whose
    // count and tick we adopt. Nothing settles until `TickSync` has
    // measured the clock: under the rough seed `hop_tick` sits a round trip
    // early.
    pub fn judge_crossing(&mut self, tick: u32, hops: u32, clock_seeded: bool) -> CrossingVerdict {
        if hops == self.hops {
            self.disputed_since = None;
            return CrossingVerdict::Paired;
        }
        let evidence = sequence_is_newer(hops, self.hops) || !sequence_is_newer(self.hop_tick, tick);
        if !evidence || !clock_seeded {
            return CrossingVerdict::Skipped;
        }
        let since = *self.disputed_since.get_or_insert(tick);
        if tick.wrapping_sub(since) < HOP_DISPUTE_SLACK_TICKS {
            return CrossingVerdict::Skipped;
        }
        self.hops = hops;
        self.hop_tick = tick;
        self.disputed_since = None;
        CrossingVerdict::Settled
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

    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut PlayerInfo> {
        self.0.values_mut()
    }

    pub fn retain(&mut self, f: impl FnMut(&PlayerId, &mut PlayerInfo) -> bool) {
        self.0.retain(f);
    }
}

// A ring of the local player's own predicted positions, one per `CMove`,
// keyed by its `seq` and tagged with our crossing count and the tick we
// simulated it at: the newest `COMMITTED_POSITION_RING_LEN` are kept, each
// slot overwritten by the sequence that lands on it a ring later. The
// server's `PlayerMove` names the slot and the count its position is
// measured against, and the tick its message carries is measured against
// the recorded one.
pub struct CommittedPositionRing([Option<CommittedPosition>; COMMITTED_POSITION_RING_LEN]);

#[derive(Clone, Copy)]
pub struct CommittedPosition {
    seq: u32,
    hops: u32,
    pub tick: u32,
    pub pos: Position,
}

impl CommittedPositionRing {
    pub fn record(&mut self, seq: u32, hops: u32, tick: u32, pos: Position) {
        self.0[Self::slot(seq)] = Some(CommittedPosition { seq, hops, tick, pos });
    }

    // The record made after `seq`, if it is still held and was on the same
    // side of the same portal crossings.
    #[must_use]
    pub fn get(&self, seq: u32, hops: u32) -> Option<CommittedPosition> {
        self.0[Self::slot(seq)].filter(|c| c.seq == seq && c.hops == hops)
    }

    #[must_use]
    pub fn tick_for_seq(&self, seq: u32) -> Option<u32> {
        self.0[Self::slot(seq)].filter(|c| c.seq == seq).map(|c| c.tick)
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
            power_ups: [true, true, false, false],
            stunned: true,
            held_keys: vec![BarrierKindId(1), BarrierKindId(3)],
            missiles: 2,
            portal_access: PortalAccess::None,
            hops: 0,
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
        assert_eq!(info.hops, player.hops);
        assert_eq!(info.hop_tick, 42);
    }

    fn crossing_info(hops: u32, hop_tick: u32) -> PlayerInfo {
        let mut info = PlayerInfo::from_snapshot(Entity::PLACEHOLDER, &snapshot_player(), hop_tick);
        info.hops = hops;
        info
    }

    #[test]
    fn a_matching_state_pairs_and_clears_a_dispute() {
        let mut info = crossing_info(3, 100);
        info.disputed_since = Some(90);

        assert_eq!(info.judge_crossing(101, 3, true), CrossingVerdict::Paired);
        assert_eq!(info.disputed_since, None);
    }

    #[test]
    fn a_state_from_before_our_crossing_is_skipped_not_evidence() {
        let mut info = crossing_info(3, 100);

        assert_eq!(info.judge_crossing(95, 2, true), CrossingVerdict::Skipped);
        assert_eq!(info.disputed_since, None);
        assert_eq!(info.hops, 3);
    }

    #[test]
    fn a_missing_crossing_settles_after_the_slack() {
        let mut info = crossing_info(3, 100);
        for tick in 100..100 + HOP_DISPUTE_SLACK_TICKS {
            assert_eq!(
                info.judge_crossing(tick, 2, true),
                CrossingVerdict::Skipped,
                "tick {tick}"
            );
        }
        assert_eq!(info.disputed_since, Some(100));

        let settled = 100 + HOP_DISPUTE_SLACK_TICKS;
        assert_eq!(info.judge_crossing(settled, 2, true), CrossingVerdict::Settled);
        assert_eq!(info.hops, 2);
        assert_eq!(info.hop_tick, settled);
        assert_eq!(info.disputed_since, None);
    }

    #[test]
    fn a_server_crossing_we_did_not_make_settles_after_the_slack() {
        let mut info = crossing_info(3, 50);

        assert_eq!(info.judge_crossing(200, 4, true), CrossingVerdict::Skipped);
        assert_eq!(info.disputed_since, Some(200));
        assert_eq!(
            info.judge_crossing(200 + HOP_DISPUTE_SLACK_TICKS, 4, true),
            CrossingVerdict::Settled
        );
        assert_eq!(info.hops, 4);
    }

    #[test]
    fn an_unseeded_clock_never_settles() {
        let mut info = crossing_info(3, 100);
        for tick in 100..200 {
            assert_eq!(info.judge_crossing(tick, 2, false), CrossingVerdict::Skipped);
        }
        assert_eq!(info.hops, 3);
        assert_eq!(info.disputed_since, None);
    }

    #[test]
    fn a_pairing_within_the_slack_ends_the_dispute() {
        let mut info = crossing_info(3, 100);
        assert_eq!(info.judge_crossing(100, 2, true), CrossingVerdict::Skipped);
        assert_eq!(info.disputed_since, Some(100));

        assert_eq!(info.judge_crossing(103, 3, true), CrossingVerdict::Paired);
        assert_eq!(info.disputed_since, None);
    }

    #[test]
    fn apply_status_updates_status_fields_only() {
        let player = snapshot_player();
        let mut info = PlayerInfo::from_snapshot(Entity::PLACEHOLDER, &player, 0);
        let status = SPlayerStatus {
            collected: None,
            id: PlayerId(12),
            power_ups: [false, false, true, false],
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
        positions.record(7, 0, 100, pos);
        let recorded = positions.get(7, 0).expect("seq 7 missing from the ring");
        assert_eq!(recorded.pos, pos);
        assert_eq!(recorded.tick, 100);
    }

    #[test]
    fn committed_position_misses_an_unrecorded_seq() {
        let mut positions = CommittedPositionRing::default();
        assert!(positions.get(0, 0).is_none());
        assert!(positions.tick_for_seq(0).is_none());
        positions.record(7, 0, 0, Position::default());
        assert!(positions.get(7 + COMMITTED_POSITION_RING_LEN as u32, 0).is_none());
        assert!(positions.tick_for_seq(7 + COMMITTED_POSITION_RING_LEN as u32).is_none());
    }

    #[test]
    fn committed_position_is_overwritten_a_ring_later() {
        let mut positions = CommittedPositionRing::default();
        positions.record(7, 0, 0, Position::default());
        positions.record(7 + COMMITTED_POSITION_RING_LEN as u32, 0, 0, Position::default());
        assert!(positions.get(7, 0).is_none());
        assert!(positions.tick_for_seq(7).is_none());
    }

    #[test]
    fn committed_position_misses_from_the_other_side_of_a_crossing() {
        let mut positions = CommittedPositionRing::default();
        positions.record(7, 1, 100, Position::default());
        assert!(positions.get(7, 0).is_none());
        assert!(positions.get(7, 1).is_some());
        assert_eq!(positions.tick_for_seq(7), Some(100));
    }

    #[test]
    fn cleared_ring_holds_nothing() {
        let mut positions = CommittedPositionRing::default();
        positions.record(7, 0, 0, Position::default());
        positions.clear();
        assert!(positions.get(7, 0).is_none());
        assert!(positions.tick_for_seq(7).is_none());
    }
}
