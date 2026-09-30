use bevy::prelude::*;

use crate::{
    cameras::MainCameraMarker,
    characters::PreviousTickPosition,
    players::{LocalMovementStep, LocalPlayerInfo, LocalPlayerMarker, eye_position},
    portals::apply_portal_view,
};
use common::{
    config::{GameplayConfig, NetworkConfig},
    physics::{CharacterVerticalVelocity, HorizontalVelocity, KnockbackVelocity, PlayerHopBody, PortalSet},
    protocol::{FaceYaw, MapSettings, PlayerMoveIntent, PlayerStance, Position},
};

pub fn portal_transit_system(
    mut commands: Commands,
    portal_set: Res<PortalSet>,
    gameplay_config: Res<GameplayConfig>,
    map_settings: Res<MapSettings>,
    network: Res<NetworkConfig>,
    mut local_player_info: ResMut<LocalPlayerInfo>,
    cameras: Query<Entity, (With<Camera3d>, With<MainCameraMarker>)>,
    mut query: Query<
        (
            &mut Position,
            &mut PreviousTickPosition,
            &mut FaceYaw,
            &mut CharacterVerticalVelocity,
            &mut PlayerMoveIntent,
            &mut KnockbackVelocity,
            &mut HorizontalVelocity,
            &mut PlayerStance,
            Option<&LocalMovementStep>,
        ),
        With<LocalPlayerMarker>,
    >,
) {
    if local_player_info.is_dead || portal_set.is_empty() {
        return;
    }
    for (
        mut pos,
        mut prev,
        mut face_yaw,
        mut vertical_velocity,
        mut move_intent,
        mut knockback,
        mut momentum,
        mut stance,
        step,
    ) in &mut query
    {
        let Some(hop) = portal_set.player_hop(
            Vec3::from(prev.0),
            Vec3::from(*pos),
            &gameplay_config,
            &map_settings.movement,
            PlayerHopBody {
                stance: *stance,
                knockback: &knockback,
                horizontal_velocity: &momentum,
                vertical_velocity: vertical_velocity.0,
                carried: step.map_or(Vec3::ZERO, |step| step.carried),
                yaw: face_yaw.0,
            },
            network.tick_duration().as_secs_f32(),
        ) else {
            continue;
        };

        let entrance = *pos;
        hop.apply_player_state(
            &mut pos,
            &mut face_yaw,
            &mut vertical_velocity,
            &mut move_intent,
            &mut stance,
        );
        hop.apply_motion_components(&mut knockback, &mut momentum);
        // Anchor render interpolation at the exit: the transit renders as a
        // cut there, not a smear between the portals.
        prev.0 = *pos;
        apply_portal_view(
            &mut commands,
            cameras.single().ok(),
            &mut local_player_info,
            eye_position(*pos, stance.eye_height(&gameplay_config.player)),
            &hop.entry,
            &hop.exit,
            hop.yaw,
        );
        local_player_info.reports.begin_crossing(entrance);
    }
}
