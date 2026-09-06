use bevy::prelude::*;
use common::{
    config::ActorMovementConfig,
    physics::{CharacterVerticalVelocity, KnockbackVelocity},
    protocol::{ActorId, ActorMarker, ActorMoveIntent, FaceYaw, PlayerMarker, Position},
};

use crate::actors::ActorCrushed;

pub(crate) type ActorMovementQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ActorId,
        &'static ActorMovementConfig,
        &'static mut Position,
        &'static mut CharacterVerticalVelocity,
        &'static mut ActorMoveIntent,
        &'static mut FaceYaw,
        Option<&'static KnockbackVelocity>,
        &'static mut ActorCrushed,
    ),
    (With<ActorMarker>, Without<PlayerMarker>),
>;
