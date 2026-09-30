use bevy::{ecs::query::QueryData, prelude::*};
use common::{
    config::ActorMovementConfig,
    physics::{CharacterSupport, CharacterVerticalVelocity, KnockbackVelocity},
    protocol::{ActorId, ActorMarker, ActorMoveIntent, FaceYaw, PlayerMarker, Position},
};

use crate::actors::{ActorCharacter, ActorCrushed, ActorLanding, SurfaceAgent};

// The movable state of an actor without surface navigation: anchored or flying.
#[derive(QueryData)]
#[query_data(mutable)]
pub struct FreeActor {
    pub entity: Entity,
    pub id: &'static ActorId,
    pub movement: Option<&'static ActorMovementConfig>,
    pub position: &'static mut Position,
    pub vertical_velocity: &'static mut CharacterVerticalVelocity,
    pub intent: &'static mut ActorMoveIntent,
    pub face_yaw: &'static mut FaceYaw,
    pub support: &'static mut CharacterSupport,
    pub knockback: Option<&'static KnockbackVelocity>,
    pub crushed: &'static mut ActorCrushed,
    pub landing: &'static mut ActorLanding,
    pub character: &'static ActorCharacter,
}

pub(crate) type FreeActorQuery<'w, 's> =
    Query<'w, 's, FreeActor, (With<ActorMarker>, Without<PlayerMarker>, Without<SurfaceAgent>)>;

// The movable state of a ground actor.
#[derive(QueryData)]
#[query_data(mutable)]
pub(crate) struct SurfaceActor {
    pub entity: Entity,
    pub id: &'static ActorId,
    pub character: &'static ActorCharacter,
    pub speeds: &'static ActorMovementConfig,
    pub agent: &'static mut SurfaceAgent,
    pub position: &'static mut Position,
    pub vertical_velocity: &'static mut CharacterVerticalVelocity,
    pub support: &'static mut CharacterSupport,
    pub intent: &'static mut ActorMoveIntent,
    pub facing: &'static mut FaceYaw,
    pub crushed: &'static mut ActorCrushed,
    pub landing: &'static mut ActorLanding,
    pub knockback: Option<&'static KnockbackVelocity>,
}
