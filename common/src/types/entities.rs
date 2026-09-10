use crate::map::Carriers;
use bevy_ecs::prelude::*;
use bincode::{Decode, Encode};

use super::{
    ActorMovementState, BarrierKindId, CarrierId, Health, ItemType, MissileMovementState, PlayerId, PlayerMoveIntent,
    PlayerMovementState, PortalAccess, Position, PowerUpKind,
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
    pub anchor: Option<ActorAnchor>,
    pub beam: Option<ActorBeam>,
    pub movement: ActorMovementState,
    pub face_yaw: f32,
    pub health: Health,
}

// The start tick identifies the burst across retargets; remaining time is measured at the enclosing message tick.
#[derive(Debug, Clone, Copy, PartialEq, Encode, Decode)]
pub struct ActorBeam {
    pub target: PlayerId,
    pub started_tick: u32,
    pub remaining_secs: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Encode, Decode)]
pub struct ActorAnchor {
    pub carrier: CarrierId,
    pub pos: Position,
}

impl ActorAnchor {
    pub fn world_position(self, carriers: &Carriers) -> Position {
        carriers.pose(self.carrier).transform_position(&self.pos)
    }
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
