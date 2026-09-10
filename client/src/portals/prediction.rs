use bevy::prelude::*;

use crate::{
    cameras::MainCameraMarker,
    characters::PreviousTickPosition,
    players::{LocalPlayerInfo, LocalPlayerMarker, PlayerMap, eye_position},
    portals::apply_portal_view,
};
use common::{
    config::GameplayConfig,
    physics::{AirborneMomentum, CharacterVerticalVelocity, KnockbackVelocity, PlayerHopBody, PortalSet},
    protocol::{FaceYaw, MapSettings, PlayerId, PlayerMoveIntent, PlayerMovementState, Position, PowerUpKind},
};

pub fn portal_transit_system(
    mut commands: Commands,
    portal_set: Res<PortalSet>,
    gameplay_config: Res<GameplayConfig>,
    map_settings: Res<MapSettings>,
    players: Res<PlayerMap>,
    mut local_player_info: ResMut<LocalPlayerInfo>,
    cameras: Query<Entity, (With<Camera3d>, With<MainCameraMarker>)>,
    mut query: Query<
        (
            Entity,
            &PlayerId,
            &mut Position,
            &mut PreviousTickPosition,
            &mut FaceYaw,
            &mut CharacterVerticalVelocity,
            &mut PlayerMoveIntent,
            Option<&mut KnockbackVelocity>,
            Option<&mut AirborneMomentum>,
        ),
        With<LocalPlayerMarker>,
    >,
) {
    if local_player_info.is_dead || portal_set.is_empty() {
        return;
    }
    for (entity, id, mut pos, mut prev, mut face_yaw, mut vertical_velocity, mut move_intent, knockback, momentum) in
        &mut query
    {
        let (has_speed, stunned) = players
            .get(id)
            .map_or((false, false), |info| (info.power_up(PowerUpKind::Speed), info.stunned));
        let Some(hop) = portal_set.player_hop(
            Vec3::from(prev.0),
            Vec3::from(*pos),
            &gameplay_config,
            &map_settings.movement,
            PlayerHopBody {
                move_intent: *move_intent,
                has_speed,
                stunned,
                knockback: knockback.as_deref(),
                airborne_momentum: momentum.as_deref(),
                vertical_velocity: vertical_velocity.0,
                yaw: face_yaw.0,
            },
        ) else {
            continue;
        };

        let entrance = PlayerMovementState::new(*pos, *move_intent, vertical_velocity.0, face_yaw.0).with_momentum(
            momentum.as_deref().map_or(Vec3::ZERO, |m| m.0),
            knockback.as_deref().map_or(Vec3::ZERO, |k| k.0),
        );
        hop.apply_player_state(&mut pos, &mut face_yaw, &mut vertical_velocity, &mut move_intent);
        hop.apply_motion_components(&mut commands, entity, knockback, momentum);
        // Anchor render interpolation at the exit: the transit renders as a
        // cut there, not a smear between the portals.
        prev.0 = *pos;
        let view_before = Vec2::new(local_player_info.stored_yaw, local_player_info.stored_pitch);
        apply_portal_view(
            &mut commands,
            cameras.single().ok(),
            &mut local_player_info,
            eye_position(*pos, gameplay_config.player.eye_height()),
            &hop.entry,
            &hop.exit,
            hop.yaw,
        );
        let view_change = Vec2::new(local_player_info.stored_yaw, local_player_info.stored_pitch) - view_before;
        let seq = local_player_info.move_seq.wrapping_add(1);
        local_player_info.portal_crossings.record(seq, entrance, view_change);
        local_player_info.committed_positions.clear();
        local_player_info.last_comparison_seq = Some(local_player_info.move_seq);
    }
}
