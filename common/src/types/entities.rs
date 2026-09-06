use bevy_ecs::prelude::*;
use bincode::{Decode, Encode};

use super::{
    ActorMoveIntent, ActorMovementState, BarrierKindId, CarrierId, Health, ItemType, MissileMovementState, PlayerId,
    PlayerMoveIntent, PlayerMovementState, PortalAccess, Position, PowerUpKind,
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
// `pos` is in the carrier's frame (world space on the world carrier), so the
// ghost rides its carrier like an item. The window is the server ticks it
// was reserved at and is due at; the client reads its shared tick against
// them, so the fade is a pure function of the tick and nothing counts down.
#[derive(Debug, Clone, Encode, Decode)]
pub struct SpawningActor {
    pub kind: String,
    pub carrier: CarrierId,
    pub pos: Position,
    pub face_yaw: f32,
    pub reserved_tick: u32,
    pub due_tick: u32,
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
    // Which portal ends this player may place. `SInit` seeds it; the snapshot
    // keeps it current as players come and go.
    pub portal_access: PortalAccess,
    // Portal crossings the player has made; seeds the client's count when
    // the player appears.
    pub hops: u32,
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
            portal_access: PortalAccess::None,
            hops: 0,
        }
    }

    #[must_use]
    pub const fn power_up(&self, kind: PowerUpKind) -> bool {
        self.power_ups[kind.index()]
    }
}

// `pos` is in the carrier's frame (world space on the world carrier), so a
// placed item rides its carrier without per-tick traffic.
#[derive(Debug, Clone, Encode, Decode, Copy)]
pub struct Item {
    pub item_type: ItemType,
    pub carrier: CarrierId,
    pub pos: Position,
}

#[derive(Debug, Clone, Copy, Encode, Decode)]
pub struct Missile {
    pub shooter: PlayerId,
    pub movement: MissileMovementState,
}
