use bincode::{Decode, Encode};

use super::{
    ActorMoveIntent, ActorMovementState, BarrierKindId, Health, ItemType, PlayerMoveIntent, PlayerMovementState,
    Position,
};

#[derive(Debug, Clone, Encode, Decode)]
pub struct Actor {
    pub kind: String,
    pub movement: ActorMovementState,
    pub face_dir: f32,
    pub health: Health,
}

impl Actor {
    #[must_use]
    pub const fn new(kind: String, pos: Position, move_intent: ActorMoveIntent, face_dir: f32, health: Health) -> Self {
        Self {
            kind,
            movement: ActorMovementState::new(pos, move_intent, 0.0),
            face_dir,
            health,
        }
    }
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct Player {
    pub name: String,
    pub movement: PlayerMovementState,
    pub face_dir: f32,
    pub health: Health,
    pub score: i32,
    pub speed_power_up: bool,
    pub multi_shot_power_up: bool,
    pub phasing_power_up: bool,
    pub anti_gravity_power_up: bool,
    pub stunned: bool,
    pub held_keys: Vec<BarrierKindId>,
}

impl Player {
    #[must_use]
    pub const fn new(
        name: String,
        pos: Position,
        move_intent: PlayerMoveIntent,
        face_dir: f32,
        score: i32,
        health: Health,
    ) -> Self {
        Self {
            name,
            movement: PlayerMovementState::new(pos, move_intent, 0.0),
            face_dir,
            health,
            score,
            speed_power_up: false,
            multi_shot_power_up: false,
            phasing_power_up: false,
            anti_gravity_power_up: false,
            stunned: false,
            held_keys: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Encode, Decode, Copy)]
pub struct Item {
    pub item_type: ItemType,
    pub pos: Position,
}
