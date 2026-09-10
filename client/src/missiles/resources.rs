use super::{MissileFlight, interpolation::RemoteMissileMotion};
use bevy::prelude::*;
use common::protocol::{HomingTarget, MissileId, PlayerId, sequence_is_newer};
use std::collections::HashMap;

#[derive(Component, Debug, Clone, Copy)]
pub struct MissileVelocity(pub Vec3);

#[derive(Component)]
pub(crate) struct OwnedMissile {
    pub flight: MissileFlight,
    pub seq: u32,
}

pub(crate) struct MissileInfo {
    pub entity: Entity,
    pub shooter: PlayerId,
    pub born_tick: u32,
    pub remote: Option<RemoteMissileMotion>,
}

#[derive(Resource, Default)]
pub struct MissileMap {
    pub(crate) entries: HashMap<MissileId, MissileInfo>,
    // Keep a detonation until later snapshots can no longer contain that missile.
    retired: HashMap<MissileId, u32>,
}

impl MissileMap {
    pub(crate) fn remove(&mut self, id: &MissileId) -> Option<MissileInfo> {
        self.entries.remove(id)
    }
    pub(crate) fn get_mut(&mut self, id: &MissileId) -> Option<&mut MissileInfo> {
        self.entries.get_mut(id)
    }
    pub(crate) fn contains_key(&self, id: &MissileId) -> bool {
        self.entries.contains_key(id)
    }
    pub(crate) fn retire(&mut self, id: MissileId, tick: u32) -> bool {
        self.retired.insert(id, tick).is_none()
    }
    pub(crate) fn is_retired(&self, id: &MissileId) -> bool {
        self.retired.contains_key(id)
    }
    pub(crate) fn discard_retired_before(&mut self, tick: u32) {
        self.retired
            .retain(|_, retired_tick| !sequence_is_newer(tick, *retired_tick));
    }
}

// What the crosshair is currently locked on, recomputed every frame by
// `lock_on_system`. `Some` only while a missile fired right now would track:
// first or third person, alive, ammo in reserve, clear muzzle, and a target
// on the aim ray with clear sight. The crosshair and weapon input read this.
#[derive(Resource, Default, PartialEq, Eq)]
pub struct LockOnTarget(pub Option<HomingTarget>);
