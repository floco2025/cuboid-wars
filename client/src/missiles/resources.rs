use bevy::prelude::*;
use std::collections::HashMap;

use common::protocol::{HomingTarget, MissileId};

// Client-side flight velocity, dead-reckoned between server updates.
#[derive(Component, Debug, Clone, Copy)]
pub struct MissileVelocity(pub Vec3);

// Map of in-flight missiles (client-side).
#[derive(Resource, Default)]
pub struct MissileMap(HashMap<MissileId, Entity>);

impl MissileMap {
    pub fn insert(&mut self, id: MissileId, entity: Entity) -> Option<Entity> {
        self.0.insert(id, entity)
    }

    pub fn remove(&mut self, id: &MissileId) -> Option<Entity> {
        self.0.remove(id)
    }

    #[must_use]
    pub fn get(&self, id: &MissileId) -> Option<Entity> {
        self.0.get(id).copied()
    }

    #[must_use]
    pub fn contains_key(&self, id: &MissileId) -> bool {
        self.0.contains_key(id)
    }

    pub fn retain(&mut self, f: impl FnMut(&MissileId, &mut Entity) -> bool) {
        self.0.retain(f);
    }
}

// What the crosshair is currently locked on, recomputed every frame by
// `lock_on_system`. `Some` only while a missile fired right now would track:
// first person, alive, ammo in reserve, target on the aim ray with clear
// sight. The crosshair color and the alt-fire gate both read this.
#[derive(Resource, Default, PartialEq, Eq)]
pub struct LockOnTarget(pub Option<HomingTarget>);
