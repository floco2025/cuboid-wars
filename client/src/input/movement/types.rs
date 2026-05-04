use bevy::prelude::*;
use common::{
    physics::CharacterVerticalVelocity,
    protocol::{FaceDirection, PlayerMoveIntent, Position},
};

use crate::players::LocalPlayerMarker;

pub(super) type LocalPlayerInputQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static Position,
        &'static mut PlayerMoveIntent,
        &'static mut FaceDirection,
        &'static mut CharacterVerticalVelocity,
    ),
    With<LocalPlayerMarker>,
>;
