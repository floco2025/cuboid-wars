use bevy::prelude::*;
use common::{
    physics::CharacterVerticalVelocity,
    protocol::{ActorId, ActorMarker, ActorMoveIntent, FaceDirection, PlayerMarker, Position},
};

pub(crate) type ActorMovementQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ActorId,
        &'static mut Position,
        &'static mut CharacterVerticalVelocity,
        &'static mut ActorMoveIntent,
        &'static mut FaceDirection,
    ),
    (With<ActorMarker>, Without<PlayerMarker>),
>;
