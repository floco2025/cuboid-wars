use bevy::prelude::*;
use common::{
    config::ActorMovementConfig,
    physics::{CharacterSupport, CharacterVerticalVelocity, KnockbackVelocity},
    protocol::{ActorId, ActorMarker, ActorMoveIntent, FaceYaw, PlayerMarker, Position},
};

use crate::actors::{ActorCharacter, ActorCrushed};

pub(crate) type ActorMovementQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ActorId,
        Option<&'static ActorMovementConfig>,
        &'static mut Position,
        &'static mut CharacterVerticalVelocity,
        &'static mut ActorMoveIntent,
        &'static mut FaceYaw,
        &'static mut CharacterSupport,
        Option<&'static KnockbackVelocity>,
        &'static mut ActorCrushed,
        &'static ActorCharacter,
    ),
    (With<ActorMarker>, Without<PlayerMarker>),
>;
