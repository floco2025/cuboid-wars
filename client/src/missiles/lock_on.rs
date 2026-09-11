use bevy::prelude::*;

use super::{MissileMarker, acquire_lock};
use crate::{
    actors::ActorMap,
    cameras::{CameraAim, CameraInputState, CameraViewMode},
    constants::{MISSILE_RADIUS, MISSILE_SPAWN_OFFSET},
    input::WeaponMode,
    missiles::LockOnTarget,
    players::{LocalPlayerInfo, MyPlayerId, PlayerMap},
    ui::ConsoleState,
};
use common::{
    config::GameplayConfig,
    physics::CollisionWorld,
    protocol::{ActorMarker, FaceYaw, HomingTarget, PlateState, PlayerMarker, Position},
};

type LockCandidateQuery<'w, 's> = Query<
    'w,
    's,
    (&'static Position, &'static FaceYaw),
    (Or<(With<PlayerMarker>, With<ActorMarker>)>, Without<MissileMarker>),
>;

// Recompute the crosshair lock every frame from the camera ray. Runs after
// the camera sync so the ray matches what the player sees (shake included).
pub fn lock_on_system(
    mut lock: ResMut<LockOnTarget>,
    view_mode: Res<CameraViewMode>,
    input: Res<CameraInputState>,
    local_player_info: Res<LocalPlayerInfo>,
    console: Res<ConsoleState>,
    my_player_id: Res<MyPlayerId>,
    players: Res<PlayerMap>,
    actors: Res<ActorMap>,
    character_data: LockCandidateQuery,
    aim: Res<CameraAim>,
    collision_world: Res<CollisionWorld>,
    gameplay_config: Res<GameplayConfig>,
    weapon_mode: Res<WeaponMode>,
    plates: Res<PlateState>,
) {
    let new_lock = compute_lock(
        &view_mode,
        &input,
        &local_player_info,
        &console,
        &my_player_id,
        &players,
        &actors,
        &character_data,
        &aim,
        &collision_world,
        &gameplay_config,
        &weapon_mode,
        &plates,
    );
    // Write only on change so `ui_crosshair_lock_system`'s `is_changed()` gate works.
    lock.set_if_neq(LockOnTarget(new_lock));
}

#[expect(clippy::too_many_arguments, reason = "pure helper over the system's full guard set")]
fn compute_lock(
    view_mode: &CameraViewMode,
    input: &CameraInputState,
    local_player_info: &LocalPlayerInfo,
    console: &ConsoleState,
    my_player_id: &MyPlayerId,
    players: &PlayerMap,
    actors: &ActorMap,
    character_data: &LockCandidateQuery,
    aim: &CameraAim,
    collision_world: &CollisionWorld,
    gameplay_config: &GameplayConfig,
    weapon_mode: &WeaponMode,
    plates: &PlateState,
) -> Option<HomingTarget> {
    if view_mode.is_top_down()
        || input.released
        || local_player_info.is_dead
        || console.open
        || *weapon_mode != WeaponMode::Missile
    {
        return None;
    }
    // Lock requires ammo: a lit crosshair always means "fire will launch".
    if players.get(&my_player_id.0).is_none_or(|info| info.missiles == 0) {
        return None;
    }

    if !collision_world.projectile_path_clear(
        aim.origin,
        aim.direction * MISSILE_SPAWN_OFFSET,
        MISSILE_RADIUS,
        &plates.open_barriers,
    ) {
        return None;
    }

    let candidates = players
        .iter()
        .filter(|(id, _)| **id != my_player_id.0)
        .filter_map(|(id, info)| {
            let (pos, face_yaw) = character_data.get(info.entity).ok()?;
            Some((
                HomingTarget::Player(*id),
                *pos,
                face_yaw.0,
                gameplay_config.player.physics(),
            ))
        })
        .chain(actors.iter().filter_map(|(id, info)| {
            let (pos, face_yaw) = character_data.get(info.entity).ok()?;
            Some((
                HomingTarget::Actor(*id),
                *pos,
                face_yaw.0,
                gameplay_config.expect_actor(&info.kind).physics(),
            ))
        }))
        .collect::<Vec<_>>();

    acquire_lock(
        collision_world,
        aim.origin,
        aim.direction,
        gameplay_config.missiles.lock_range,
        gameplay_config.missiles.lock_assist_radius,
        candidates.into_iter(),
    )
}
