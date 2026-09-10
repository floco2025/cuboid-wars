use bevy::prelude::*;
use common::{
    map::Carriers,
    protocol::{FaceYaw, PlayerId, PlayerMarker, Position},
};

use super::PlayerMap;

// Riders keep their carrier-local report, so the world position follows the carrier every tick.
pub(crate) fn apply_player_movement_system(
    players: Res<PlayerMap>,
    carriers: Res<Carriers>,
    mut query: Query<(&PlayerId, &mut Position, &mut FaceYaw), With<PlayerMarker>>,
) {
    for (id, mut pos, mut yaw) in &mut query {
        let Some(info) = players.get(id).filter(|info| !info.is_dead()) else {
            continue;
        };
        let movement = &info.life.movement;
        *pos = carriers.pose(movement.carrier).transform_position(&movement.pos);
        yaw.0 = movement.face_yaw;
    }
}
