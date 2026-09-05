use bevy::prelude::*;

use crate::{
    cameras::MainCameraMarker,
    characters::PreviousTickPosition,
    players::{LocalPlayerInfo, MyPlayerId, PlayerMap},
    portals::apply_portal_view,
};
use common::{
    config::GameplayConfig,
    physics::{CharacterVerticalVelocity, KnockbackVelocity, PortalMomentum, PortalSet},
    protocol::{FaceYaw, MapSettings, PlayerId, PlayerMarker, PlayerMoveIntent, Position, PowerUpKind},
};

// Portal transit for every simulated player, local and remote alike — the
// same shared crossing the server computes, run right after this tick's
// movement. A crossing is derived state, not an input event: the shared
// geometry (placements arrive via `SPortalOpened`) plus the motion this
// client already simulates determine it, so there is no teleport message.
// A wrong guess about a remote player's motion near a plane surfaces as an
// ordinary snapshot correction.
pub fn portal_transit_system(
    mut commands: Commands,
    portal_set: Res<PortalSet>,
    gameplay_config: Res<GameplayConfig>,
    map_settings: Res<MapSettings>,
    my_player_id: Res<MyPlayerId>,
    mut players: ResMut<PlayerMap>,
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
            Option<&mut PortalMomentum>,
        ),
        With<PlayerMarker>,
    >,
) {
    if portal_set.is_empty() {
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
            *move_intent,
            has_speed,
            stunned,
            knockback.as_deref(),
            momentum.as_deref(),
            vertical_velocity.0,
            face_yaw.0,
        ) else {
            continue;
        };

        hop.apply_player_state(&mut pos, &mut face_yaw, &mut vertical_velocity, &mut move_intent);
        hop.apply_motion_components(&mut commands, entity, knockback, momentum);
        // Anchor render interpolation at the exit: the transit renders as a
        // cut there, not a smear between the portals.
        prev.0 = *pos;
        if let Some(info) = players.get_mut(id) {
            info.hops = info.hops.wrapping_add(1);
        }
        if my_player_id.0 == *id {
            apply_portal_view(
                &mut commands,
                cameras.single().ok(),
                &mut local_player_info,
                Vec3::new(pos.x, pos.y + gameplay_config.player.eye_height(), pos.z),
                &hop.entry,
                &hop.exit,
                hop.yaw,
            );
        }
    }
}
