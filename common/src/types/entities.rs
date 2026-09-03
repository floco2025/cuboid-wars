use bevy_ecs::prelude::*;
use bincode::{Decode, Encode};

use super::{
    ActorMoveIntent, ActorMovementState, BarrierKindId, Health, ItemType, MissileMovementState, PlayerId,
    PlayerMoveIntent, PlayerMovementState, Position, PowerUpKind,
};

// Marker components disambiguating entity archetypes across server and client.
#[derive(Component, Debug, Default)]
pub struct PlayerMarker;

#[derive(Component, Debug, Default)]
pub struct ActorMarker;

#[derive(Component, Debug, Default)]
pub struct ItemMarker;

#[derive(Component, Debug, Default)]
pub struct MissileMarker;

#[derive(Component, Debug, Default)]
pub struct ProjectileMarker;

#[derive(Debug, Clone, Encode, Decode)]
pub struct Actor {
    pub kind: String,
    pub movement: ActorMovementState,
    pub face_yaw: f32,
    pub health: Health,
}

// A reserved actor spawn during its warning window. The actor doesn't exist
// yet — clients render a purely visual beam-in ghost at the reserved spot.
// `remaining_secs` carries the fade state; the full warning duration is static
// bootstrap data, so snapshots only resynchronize the changing value.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SpawningActor {
    pub kind: String,
    pub pos: Position,
    pub face_yaw: f32,
    pub remaining_secs: f32,
}

impl Actor {
    #[must_use]
    pub const fn new(kind: String, pos: Position, move_intent: ActorMoveIntent, face_yaw: f32, health: Health) -> Self {
        Self {
            kind,
            movement: ActorMovementState::new(pos, move_intent, 0.0),
            face_yaw,
            health,
        }
    }
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct Player {
    pub name: String,
    // Carries facing too (`face_yaw`) — no separate field.
    pub movement: PlayerMovementState,
    pub health: Health,
    pub score: i32,
    // One bool per `PowerUpKind`, indexed by `PowerUpKind::index()`.
    pub power_ups: [bool; PowerUpKind::COUNT],
    pub stunned: bool,
    pub held_keys: Vec<BarrierKindId>,
    pub missiles: u32,
}

impl Player {
    #[must_use]
    pub const fn new(
        name: String,
        pos: Position,
        move_intent: PlayerMoveIntent,
        face_yaw: f32,
        score: i32,
        health: Health,
    ) -> Self {
        Self {
            name,
            movement: PlayerMovementState::new(pos, move_intent, 0.0, face_yaw),
            health,
            score,
            power_ups: [false; PowerUpKind::COUNT],
            stunned: false,
            held_keys: Vec::new(),
            missiles: 0,
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

#[derive(Debug, Clone, Copy, Encode, Decode)]
pub struct Missile {
    pub shooter: PlayerId,
    pub movement: MissileMovementState,
}
