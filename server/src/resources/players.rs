use std::collections::HashMap;

use bevy::prelude::*;
use tokio::sync::mpsc::UnboundedSender;

use crate::net::ServerToClient;
use common::{
    constants::{ALWAYS_ANTI_GRAVITY, ALWAYS_MULTI_SHOT, ALWAYS_PHASING, ALWAYS_SPEED},
    protocol::{BarrierKindId, PlayerId, SPlayerStatus},
};

pub struct PlayerInfo {
    pub entity: Entity,
    pub logged_in: bool,
    pub channel: UnboundedSender<ServerToClient>,
    pub score: i32,
    pub name: String,
    pub speed_power_up_timer: f32,
    pub multi_shot_power_up_timer: f32,
    pub phasing_power_up_timer: f32,
    pub anti_gravity_power_up_timer: f32,
    pub stun_timer: f32,
    pub last_shot_time: f32,
    // Permanent inventory: a key, once collected, stays held. Kept sorted
    // ascending so the encoded `SPlayerStatus` bytes are deterministic and
    // the client can change-detect via a single equality check.
    pub held_keys: Vec<BarrierKindId>,
    // `Some(remaining_secs)` from the moment a player's health drops to zero
    // until the respawn system spawns a new entity. While dead, the player
    // has no entity and is absent from `SUpdate` — the local client sees
    // this as "I disappeared from the snapshot" and shows the death overlay.
    pub death_timer: Option<f32>,
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
            speed_power_up_timer: 0.0,
            multi_shot_power_up_timer: 0.0,
            phasing_power_up_timer: 0.0,
            anti_gravity_power_up_timer: 0.0,
            stun_timer: 0.0,
            last_shot_time: f32::NEG_INFINITY,
            held_keys: Vec::new(),
            death_timer: None,
        }
    }

    #[must_use]
    pub fn is_dead(&self) -> bool {
        self.death_timer.is_some()
    }

    // Clear per-life state that should not persist across a death: power-ups,
    // stun, and held keys. Called at death (not respawn — once cleared, the
    // player respawns with no power-ups regardless of when respawn fires).
    pub fn clear_per_life_state(&mut self) {
        self.speed_power_up_timer = 0.0;
        self.multi_shot_power_up_timer = 0.0;
        self.phasing_power_up_timer = 0.0;
        self.anti_gravity_power_up_timer = 0.0;
        self.stun_timer = 0.0;
        self.held_keys.clear();
    }

    #[must_use]
    pub fn has_key(&self, kind: BarrierKindId) -> bool {
        self.held_keys.binary_search(&kind).is_ok()
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
    pub fn has_speed(&self) -> bool {
        ALWAYS_SPEED || self.speed_power_up_timer > 0.0
    }

    #[must_use]
    pub fn has_multi_shot(&self) -> bool {
        ALWAYS_MULTI_SHOT || self.multi_shot_power_up_timer > 0.0
    }

    #[must_use]
    pub fn has_phasing(&self) -> bool {
        ALWAYS_PHASING || self.phasing_power_up_timer > 0.0
    }

    #[must_use]
    pub fn has_anti_gravity(&self) -> bool {
        ALWAYS_ANTI_GRAVITY || self.anti_gravity_power_up_timer > 0.0
    }

    #[must_use]
    pub fn status(&self, id: PlayerId) -> SPlayerStatus {
        SPlayerStatus {
            id,
            speed_power_up: self.has_speed(),
            multi_shot_power_up: self.has_multi_shot(),
            phasing_power_up: self.has_phasing(),
            anti_gravity_power_up: self.has_anti_gravity(),
            stunned: self.stun_timer > 0.0,
            held_keys: self.held_keys.clone(),
        }
    }

    pub fn tick_timers(&mut self, delta: f32) {
        tick_timer(&mut self.speed_power_up_timer, delta);
        tick_timer(&mut self.multi_shot_power_up_timer, delta);
        tick_timer(&mut self.phasing_power_up_timer, delta);
        tick_timer(&mut self.anti_gravity_power_up_timer, delta);
        tick_timer(&mut self.stun_timer, delta);
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
}
