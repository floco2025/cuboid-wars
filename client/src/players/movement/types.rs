use bevy::prelude::*;
use common::{
    physics::{CharacterVerticalVelocity, KnockbackVelocity},
    protocol::{ActorMarker, PlayerId, PlayerMarker, PlayerMoveIntent, Position},
};

use crate::{
    network::ServerReconciliation,
    players::{BumpFeedbackState, LocalPlayerMarker},
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
        Option<&'static mut BumpFeedbackState>,
        Option<&'static mut ServerReconciliation>,
        Option<&'static KnockbackVelocity>,
        Has<LocalPlayerMarker>,
    ),
    (With<PlayerMarker>, Without<ActorMarker>),
>;
