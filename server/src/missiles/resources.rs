use bevy::prelude::*;
use common::protocol::{Missile, MissileId, PlayerId, Position};
use std::collections::HashMap;

struct MissileEntry {
    missile: Missile,
    launched_tick: u32,
}

#[derive(Resource, Default)]
pub struct MissileMap {
    map: HashMap<MissileId, MissileEntry>,
    next_id: u32,
}

impl MissileMap {
    pub fn allocate(&mut self) -> MissileId {
        let id = MissileId(self.next_id);
        self.next_id = self.next_id.wrapping_add(1);
        id
    }
    pub fn insert(&mut self, id: MissileId, missile: Missile, launched_tick: u32) {
        self.map.insert(id, MissileEntry { missile, launched_tick });
    }
    pub fn remove(&mut self, id: &MissileId) -> Option<Missile> {
        self.map.remove(id).map(|entry| entry.missile)
    }
    pub fn get(&self, id: &MissileId) -> Option<&Missile> {
        self.map.get(id).map(|entry| &entry.missile)
    }
    pub fn get_mut(&mut self, id: &MissileId) -> Option<&mut Missile> {
        self.map.get_mut(id).map(|entry| &mut entry.missile)
    }
    pub fn iter(&self) -> impl Iterator<Item = (&MissileId, &Missile)> {
        self.map.iter().map(|(id, entry)| (id, &entry.missile))
    }
    pub fn remove_shooter(&mut self, shooter: PlayerId) {
        self.map.retain(|_, entry| entry.missile.shooter != shooter);
    }
    // Removes every missile launched more than `max_age_ticks` before `tick`, with its last reported position.
    pub fn take_expired(&mut self, tick: u32, max_age_ticks: u32) -> Vec<(MissileId, Position)> {
        let mut expired = Vec::new();
        self.map.retain(|id, entry| {
            if tick.wrapping_sub(entry.launched_tick) <= max_age_ticks {
                return true;
            }
            expired.push((*id, entry.missile.movement.pos));
            false
        });
        expired.sort_by_key(|(id, _)| id.0);
        expired
    }
}
