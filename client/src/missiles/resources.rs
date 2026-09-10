use bevy::prelude::*;
use common::protocol::{HomingTarget, MissileId, PlayerId, sequence_is_newer};
use std::collections::{HashMap, HashSet};

pub(crate) struct MissileInfo {
    pub entity: Entity,
    pub shooter: PlayerId,
    pub born_tick: u32,
}

#[derive(Resource, Default)]
pub struct MissileMap {
    entries: HashMap<MissileId, MissileInfo>,
    // Keep a detonation until later snapshots can no longer contain that missile.
    retired: HashMap<MissileId, u32>,
    // Own flights that detonated here; the server's cue for them needs no effect.
    ended_locally: HashSet<MissileId>,
}

impl MissileMap {
    pub(crate) fn insert(&mut self, id: MissileId, info: MissileInfo) {
        self.entries.insert(id, info);
    }

    // The shooter's own flight ended here.
    pub(crate) fn remove(&mut self, id: &MissileId) -> Option<MissileInfo> {
        let info = self.entries.remove(id)?;
        self.ended_locally.insert(*id);
        Some(info)
    }

    pub(crate) fn take(&mut self, id: &MissileId) -> Option<MissileInfo> {
        self.entries.remove(id)
    }

    pub(crate) fn get(&self, id: &MissileId) -> Option<&MissileInfo> {
        self.entries.get(id)
    }

    pub(crate) fn contains_key(&self, id: &MissileId) -> bool {
        self.entries.contains_key(id)
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (&MissileId, &MissileInfo)> {
        self.entries.iter()
    }

    // Whether the detonation cue at `tick` is the first word of this missile's end.
    pub(crate) fn retire(&mut self, id: MissileId, tick: u32) -> bool {
        let ended_here = self.ended_locally.remove(&id);
        self.retired.insert(id, tick).is_none() && !ended_here
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
