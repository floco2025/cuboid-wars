use std::collections::{HashMap, VecDeque};

use bevy::prelude::*;

use common::protocol::{ActorId, ActorMoveIntent, Position};

pub struct ActorInfo {
    pub entity: Entity,
    pub spawn_zone_index: usize,
    pub spawn_kind: String,
    pub direction_timer: f32,
    pub patrol_intent: ActorMoveIntent,
    pub go_to_position: Option<Position>,
    pub go_to_position_is_chase: bool,
    pub return_path: VecDeque<Position>,
    pub chase_reacquire_timer: f32,
    pub wall_avoidance_direction: Option<f32>,
    pub last_broadcast_move_intent: ActorMoveIntent,
    pub move_intent_send_timer: f32,
}

#[derive(Resource, Default)]
pub struct ActorMap(HashMap<ActorId, ActorInfo>);

impl ActorMap {
    pub fn insert(&mut self, id: ActorId, info: ActorInfo) -> Option<ActorInfo> {
        self.0.insert(id, info)
    }

    pub fn remove(&mut self, id: &ActorId) -> Option<ActorInfo> {
        self.0.remove(id)
    }

    #[must_use]
    pub fn get(&self, id: &ActorId) -> Option<&ActorInfo> {
        self.0.get(id)
    }

    pub fn get_mut(&mut self, id: &ActorId) -> Option<&mut ActorInfo> {
        self.0.get_mut(id)
    }

    pub fn values(&self) -> impl Iterator<Item = &ActorInfo> {
        self.0.values()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&ActorId, &ActorInfo)> {
        self.0.iter()
    }

    #[must_use]
    pub fn entity_is_actor(&self, entity: Entity) -> bool {
        self.0.values().any(|actor| actor.entity == entity)
    }

    #[must_use]
    pub fn info_for_entity(&self, entity: Entity) -> Option<&ActorInfo> {
        self.0.values().find(|actor| actor.entity == entity)
    }
}

#[derive(Resource, Default)]
pub struct ActorSpawner {
    pub next_id: u32,
}

#[derive(Resource, Default)]
pub struct ActorSpawnThrottles(pub HashMap<usize, f32>);

impl ActorSpawner {
    pub fn allocate(&mut self) -> ActorId {
        let id = ActorId(self.next_id);
        self.next_id = self.next_id.wrapping_add(1);
        id
    }
}
