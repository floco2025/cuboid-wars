use bevy::prelude::*;
use common::{
    physics::CharacterVerticalVelocity,
    protocol::{ActorMarker, PlayerId, PlayerMarker, PlayerMoveIntent, Position},
};

use crate::{
    network::ServerReconciliation,
    players::{BumpFlashState, LocalPlayerMarker},
};

pub(crate) type PlayerMovementQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static PlayerId,
        &'static mut Position,
        &'static PlayerMoveIntent,
        &'static mut CharacterVerticalVelocity,
        Option<&'static mut BumpFlashState>,
        Option<&'static mut ServerReconciliation>,
        Has<LocalPlayerMarker>,
    ),
    (With<PlayerMarker>, Without<ActorMarker>),
>;
