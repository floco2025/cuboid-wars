use bevy::prelude::*;
use common::{
    physics::{CharacterVerticalVelocity, KnockbackVelocity},
    protocol::{ActorId, ActorMarker, ActorMoveIntent, FaceYaw, PlayerMarker, Position},
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
        &'static mut FaceYaw,
        Option<&'static KnockbackVelocity>,
    ),
    (With<ActorMarker>, Without<PlayerMarker>),
>;
