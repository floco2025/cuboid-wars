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
    pub hits: i32,
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
}

impl PlayerInfo {
    #[must_use]
    pub fn new(entity: Entity, channel: UnboundedSender<ServerToClient>) -> Self {
        Self {
            entity,
            logged_in: false,
            channel,
            hits: 0,
            name: String::new(),
            speed_power_up_timer: 0.0,
            multi_shot_power_up_timer: 0.0,
            phasing_power_up_timer: 0.0,
            anti_gravity_power_up_timer: 0.0,
            stun_timer: 0.0,
            last_shot_time: f32::NEG_INFINITY,
            held_keys: Vec::new(),
        }
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
