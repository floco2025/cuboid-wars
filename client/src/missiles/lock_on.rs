use bevy::{
    prelude::*,
    window::{CursorGrabMode, CursorOptions},
};

use crate::{
    actors::ActorMap,
    cameras::{CameraViewMode, MainCameraMarker},
    input::WeaponMode,
    missiles::LockOnTarget,
    players::{LocalPlayerInfo, MyPlayerId, PlayerMap},
    ui::ConsoleState,
};
use common::{
    config::GameplayConfig,
    physics::{CollisionWorld, acquire_lock},
    protocol::{ActorMarker, BridgeKindId, FaceYaw, HomingTarget, MissileMarker, PlateState, PlayerMarker, Position},
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
    cursor_options: Single<&CursorOptions>,
    local_player_info: Res<LocalPlayerInfo>,
    console: Res<ConsoleState>,
    my_player_id: Res<MyPlayerId>,
    players: Res<PlayerMap>,
    actors: Res<ActorMap>,
    character_data: LockCandidateQuery,
    camera_query: Query<&Transform, (With<Camera3d>, With<MainCameraMarker>)>,
    collision_world: Res<CollisionWorld>,
    plates: Res<PlateState>,
    gameplay_config: Res<GameplayConfig>,
    weapon_mode: Res<WeaponMode>,
) {
    let new_lock = compute_lock(
        &view_mode,
        &cursor_options,
        &local_player_info,
        &console,
        &my_player_id,
        &players,
        &actors,
        &character_data,
        &camera_query,
        &collision_world,
        &plates.powered_bridge_kinds,
        &gameplay_config,
        &weapon_mode,
    );
    // Write only on change so `ui_crosshair_lock_system`'s `is_changed()` gate works.
    lock.set_if_neq(LockOnTarget(new_lock));
}

#[expect(clippy::too_many_arguments, reason = "pure helper over the system's full guard set")]
fn compute_lock(
    view_mode: &CameraViewMode,
    cursor_options: &CursorOptions,
    local_player_info: &LocalPlayerInfo,
    console: &ConsoleState,
    my_player_id: &MyPlayerId,
    players: &PlayerMap,
    actors: &ActorMap,
    character_data: &LockCandidateQuery,
    camera_query: &Query<&Transform, (With<Camera3d>, With<MainCameraMarker>)>,
    collision_world: &CollisionWorld,
    powered_bridges: &[BridgeKindId],
    gameplay_config: &GameplayConfig,
    weapon_mode: &WeaponMode,
) -> Option<HomingTarget> {
    if !view_mode.is_first_person()
        || cursor_options.grab_mode == CursorGrabMode::None
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
    let camera = camera_query.iter().next()?;

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
        powered_bridges,
        camera.translation,
        *camera.forward(),
        gameplay_config.missiles.lock_range,
        gameplay_config.missiles.lock_assist_radius,
        candidates.into_iter(),
    )
}
