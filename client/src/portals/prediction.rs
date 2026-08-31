use bevy::prelude::*;

use crate::{
    cameras::MainCameraMarker,
    characters::PreviousTickPosition,
    players::{LocalPlayerInfo, MyPlayerId, PlayerMap},
    portals::apply_portal_view,
};
use common::{
    config::GameplayConfig,
    constants::PORTAL_KNOCKBACK_CARRY_FACTOR,
    physics::{CharacterVerticalVelocity, KnockbackVelocity, PortalSet, player_control_velocity},
    protocol::{FaceYaw, PlayerId, PlayerMarker, PlayerMoveIntent, Position, PowerUpKind},
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
    time: Res<Time>,
    portal_set: Res<PortalSet>,
    gameplay_config: Res<GameplayConfig>,
    my_player_id: Option<Res<MyPlayerId>>,
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
            &PlayerMoveIntent,
            Option<&mut KnockbackVelocity>,
        ),
        With<PlayerMarker>,
    >,
) {
    if portal_set.is_empty() {
        return;
    }
    let knockback_cap = PORTAL_KNOCKBACK_CARRY_FACTOR * gameplay_config.movement.knockback.max_speed;
    for (entity, id, mut pos, mut prev, mut face_yaw, mut vertical_velocity, move_intent, knockback) in &mut query {
        let (has_speed, stunned) = players
            .get(id)
            .map_or((false, false), |info| (info.power_up(PowerUpKind::Speed), info.stunned));
        let control = player_control_velocity(*move_intent, &gameplay_config, has_speed, stunned);
        let knockback_velocity = knockback.as_ref().map_or(Vec3::ZERO, |k| k.0);
        let Some(hop) = portal_set.character_hop(
            Vec3::from(prev.0),
            Vec3::from(*pos),
            gameplay_config.player.physics(),
            control,
            knockback_velocity,
            vertical_velocity.0,
            face_yaw.0,
            knockback_cap,
        ) else {
            continue;
        };

        *pos = hop.origin.into();
        // Anchor render interpolation at the exit: the transit renders as a
        // cut there, not a smear between the portals.
        prev.0 = *pos;
        face_yaw.0 = hop.yaw;
        vertical_velocity.0 = hop.vertical_velocity;
        match knockback {
            Some(mut existing) => existing.0 = hop.knockback,
            None => {
                commands.entity(entity).insert(KnockbackVelocity(hop.knockback));
            }
        }
        if let Some(info) = players.get_mut(id) {
            // Snapshot data built before the crossing would drag this player
            // back to a stale phase; reconciliation stands down briefly.
            info.last_teleport_time = time.elapsed_secs();
        }
        if my_player_id.as_ref().is_some_and(|my| my.0 == *id) {
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
