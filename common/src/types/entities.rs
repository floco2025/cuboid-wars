use bincode::{Decode, Encode};

use super::{
    ActorMoveIntent, ActorMovementState, BarrierKindId, Health, ItemType, PlayerMoveIntent, PlayerMovementState,
    Position, PowerUpKind,
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
    // One bool per `PowerUpKind`, indexed by `PowerUpKind::index()`.
    pub power_ups: [bool; PowerUpKind::COUNT],
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
            power_ups: [false; PowerUpKind::COUNT],
            stunned: false,
            held_keys: Vec::new(),
        }
    }

    #[must_use]
    pub const fn power_up(&self, kind: PowerUpKind) -> bool {
        self.power_ups[kind.index()]
    }
}

#[derive(Debug, Clone, Encode, Decode, Copy)]
pub struct Item {
    pub item_type: ItemType,
    pub pos: Position,
}
