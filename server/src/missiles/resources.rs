use bevy::prelude::*;
use common::protocol::{Missile, MissileId, PlayerId};
use std::collections::HashMap;

#[derive(Resource, Default)]
pub struct MissileMap {
    map: HashMap<MissileId, Missile>,
    next_id: u32,
}

impl MissileMap {
    pub fn allocate(&mut self) -> MissileId {
        let id = MissileId(self.next_id);
        self.next_id = self.next_id.wrapping_add(1);
        id
    }
    pub fn insert(&mut self, id: MissileId, missile: Missile) {
        self.map.insert(id, missile);
    }
    pub fn remove(&mut self, id: &MissileId) -> Option<Missile> {
        self.map.remove(id)
    }
    pub fn get(&self, id: &MissileId) -> Option<&Missile> {
        self.map.get(id)
    }
    pub fn get_mut(&mut self, id: &MissileId) -> Option<&mut Missile> {
        self.map.get_mut(id)
    }
    pub fn iter(&self) -> impl Iterator<Item = (&MissileId, &Missile)> {
        self.map.iter()
    }
    pub fn remove_shooter(&mut self, shooter: PlayerId) {
        self.map.retain(|_, missile| missile.shooter != shooter);
    }
}
