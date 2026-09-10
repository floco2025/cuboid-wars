use super::PlayerMap;
use bevy::prelude::*;
use common::{
    map::Carriers,
    physics::{AirborneMomentum, CharacterVerticalVelocity, KnockbackVelocity},
    protocol::{FaceYaw, PlayerId, PlayerMarker, PlayerMoveIntent, Position},
};

pub(crate) fn apply_player_movement_system(
    mut players: ResMut<PlayerMap>,
    carriers: Res<Carriers>,
    mut query: Query<
        (
            &PlayerId,
            &mut Position,
            &mut FaceYaw,
            &mut CharacterVerticalVelocity,
            &mut PlayerMoveIntent,
            &mut AirborneMomentum,
            &mut KnockbackVelocity,
        ),
        With<PlayerMarker>,
    >,
) {
    for (id, mut pos, mut yaw, mut vertical, mut intent, mut momentum, mut knockback) in &mut query {
        let Some(info) = players.get_mut(id).filter(|info| !info.is_dead()) else {
            continue;
        };
        let Some(report) = info.life.movement_report.as_ref() else {
            continue;
        };
        info.life.portal_crossing = report.portal_crossing;
        let movement = report.movement;
        *pos = carriers.pose(movement.carrier).transform_position(&movement.pos);
        *intent = movement.move_intent;
        yaw.0 = movement.face_yaw;
        vertical.0 = movement.vertical_velocity;
        momentum.0 = Vec3::from_array(movement.airborne_momentum);
        knockback.0 = Vec3::from_array(movement.knockback);
        info.life.support = movement.support;
    }
}
