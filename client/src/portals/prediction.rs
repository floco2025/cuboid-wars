use bevy::prelude::*;

use crate::{
    cameras::MainCameraMarker,
    characters::PreviousTickPosition,
    players::{LocalPlayerInfo, LocalPlayerMarker, MyPlayerId, PlayerMap},
    portals::apply_portal_view,
};
use common::{
    config::GameplayConfig,
    constants::PORTAL_KNOCKBACK_CARRY_FACTOR,
    physics::{CharacterVerticalVelocity, KnockbackVelocity, PortalSet, player_control_velocity},
    protocol::{FaceYaw, PlayerMoveIntent, Position, PowerUpKind},
};

// Local-player teleport prediction: the same shared crossing test the server
// runs, applied right after this tick's predicted movement — the body sank
// into the aperture (backing colliders excluded) and continues from the
// paired end with no round-trip stall. `SPlayerTeleport` then confirms
// silently or hard-corrects (a portal re-shot mid-crossing).
pub fn local_player_portal_prediction_system(
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
            &mut Position,
            &mut PreviousTickPosition,
            &mut FaceYaw,
            &mut CharacterVerticalVelocity,
            &PlayerMoveIntent,
            Option<&mut KnockbackVelocity>,
        ),
        With<LocalPlayerMarker>,
    >,
) {
    if portal_set.is_empty() {
        return;
    }
    let Ok((entity, mut pos, mut prev, mut face_yaw, mut vertical_velocity, move_intent, knockback)) =
        query.single_mut()
    else {
        return;
    };
    let Some(my_id) = my_player_id else {
        return;
    };
    let (has_speed, stunned) = players
        .get(&my_id.0)
        .map_or((false, false), |info| (info.power_up(PowerUpKind::Speed), info.stunned));
    let control = player_control_velocity(*move_intent, &gameplay_config, has_speed, stunned);
    let knockback_velocity = knockback.as_ref().map_or(Vec3::ZERO, |k| k.0);
    let knockback_cap = PORTAL_KNOCKBACK_CARRY_FACTOR * gameplay_config.movement.knockback.max_speed;
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
        return;
    };

    *pos = hop.origin.into();
    // Anchor render interpolation at the exit: the transit renders as a cut
    // there, not a smear between the portals.
    prev.0 = *pos;
    face_yaw.0 = hop.yaw;
    vertical_velocity.0 = hop.vertical_velocity;
    match knockback {
        Some(mut existing) => existing.0 = hop.knockback,
        None => {
            commands.entity(entity).insert(KnockbackVelocity(hop.knockback));
        }
    }
    apply_portal_view(
        &mut commands,
        cameras.single().ok(),
        &mut local_player_info,
        Vec3::new(pos.x, pos.y + gameplay_config.player.eye_height(), pos.z),
        &hop.entry,
        &hop.exit,
        hop.yaw,
    );
    let now = time.elapsed_secs();
    local_player_info.predicted_teleport_time = now;
    local_player_info.predicted_teleport_pos = Vec3::from(*pos);
    if let Some(info) = players.get_mut(&my_id.0) {
        // Reconciliation stands down exactly as if the cue had landed.
        info.last_teleport_time = now;
    }
}
