use crate::players::PlayerMap;
use bevy::prelude::*;
use common::{
    config::GameplayConfig,
    physics::{AirborneMomentum, CharacterVerticalVelocity, KnockbackVelocity, PlayerHopBody, PortalSet},
    protocol::{FaceYaw, MapSettings, PlayerId, PlayerMarker, PlayerMoveIntent, Position},
};

pub fn players_portal_traversal_system(
    mut commands: Commands,
    portal_set: Res<PortalSet>,
    mut players: ResMut<PlayerMap>,
    gameplay: Res<GameplayConfig>,
    settings: Res<MapSettings>,
    mut query: Query<
        (
            Entity,
            &PlayerId,
            &mut Position,
            &mut FaceYaw,
            &mut CharacterVerticalVelocity,
            &mut PlayerMoveIntent,
            Option<&mut KnockbackVelocity>,
            Option<&mut AirborneMomentum>,
        ),
        With<PlayerMarker>,
    >,
) {
    if portal_set.is_empty() {
        return;
    }
    for (entity, id, mut pos, mut yaw, mut vertical, mut intent, knockback, airborne) in &mut query {
        let Some(info) = players.get_mut(id) else {
            continue;
        };
        let Some(path) = &info.life.movement_path else {
            continue;
        };
        let Some(hop) = portal_set.player_hop(
            Vec3::from(path.start),
            Vec3::from(*pos),
            &gameplay,
            &settings.movement,
            PlayerHopBody {
                move_intent: *intent,
                has_speed: info.has_speed(),
                stunned: info.is_stunned(),
                knockback: knockback.as_deref(),
                airborne_momentum: airborne.as_deref(),
                vertical_velocity: vertical.0,
                yaw: yaw.0,
            },
        ) else {
            continue;
        };
        if let Some(path) = &mut info.life.movement_path {
            path.portal_entry = Some(*pos);
        }
        hop.apply_player_state(&mut pos, &mut yaw, &mut vertical, &mut intent);
        hop.apply_motion_components(&mut commands, entity, knockback, airborne);
        info.session.hops = info.session.hops.wrapping_add(1);
    }
}
