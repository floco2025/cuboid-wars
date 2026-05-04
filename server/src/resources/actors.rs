use std::collections::HashMap;

use bevy::prelude::*;

use common::protocol::{ActorId, ActorMoveIntent, Position};

pub struct ActorInfo {
    pub entity: Entity,
    pub spawn_zone_index: usize,
    pub spawn_kind: String,
    pub direction_timer: f32,
    pub patrol_intent: ActorMoveIntent,
    pub go_to_position: Option<Position>,
    pub wall_avoidance_direction: Option<f32>,
    pub last_broadcast_move_intent: ActorMoveIntent,
    pub move_intent_send_timer: f32,
}

#[derive(Resource, Default)]
pub struct ActorMap(pub HashMap<ActorId, ActorInfo>);

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
