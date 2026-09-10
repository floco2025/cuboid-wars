use bevy::prelude::*;

use super::{
    CameraViewMode, FollowCamera, MainCameraMarker, TopDownCameraYaw,
    third_person::third_person_transform,
    top_down::{topdown_camera_transform, window_aspect_ratio},
};
use crate::{
    characters::PreviousTickPosition,
    config::ClientSettings,
    players::{CameraShake, LocalPlayerInfo, LocalPlayerMarker, eye_position},
};
use common::{
    config::GameplayConfig,
    physics::CollisionWorld,
    protocol::{MapLayout, MapSettings, Position},
};

// Update camera position to follow local player. Physics ticks at 30 Hz;
// interpolate between last-tick and current-tick positions so the camera
// stays smooth at the render rate.
pub fn local_player_camera_sync_system(
    local_player_query: Query<(&Position, &PreviousTickPosition), With<LocalPlayerMarker>>,
    map_layout: Res<MapLayout>,
    map_settings: Res<MapSettings>,
    windows: Query<&Window>,
    fixed_time: Res<Time<Fixed>>,
    mut camera_query: Query<
        (&mut Transform, &mut Projection, Option<&CameraShake>),
        (With<Camera3d>, With<MainCameraMarker>),
    >,
    mut view_mode: ResMut<CameraViewMode>,
    top_down_camera_yaw: Res<TopDownCameraYaw>,
    client_settings: Res<ClientSettings>,
    gameplay_config: Res<GameplayConfig>,
    local_player_info: Res<LocalPlayerInfo>,
    collision_world: Res<CollisionWorld>,
    mut third: ResMut<FollowCamera>,
    time: Res<Time>,
) {
    let Some((current_pos, prev_pos)) = local_player_query.iter().next() else {
        return;
    };
    let interp = prev_pos.lerp_to(*current_pos, fixed_time.overstep_fraction());
    let interpolated = Position {
        x: interp.x,
        y: interp.y,
        z: interp.z,
    };
    let player_pos = &interpolated;

    let Ok((mut camera_transform, mut projection, maybe_shake)) = camera_query.single_mut() else {
        return;
    };

    let Projection::Perspective(persp) = projection.as_mut() else {
        return;
    };

    persp.fov = if view_mode.is_top_down() {
        client_settings.camera.top_down.fov_degrees
    } else {
        client_settings.preferences.fov_degrees
    }
    .to_radians();

    if view_mode.is_top_down() {
        *camera_transform = topdown_camera_transform(
            player_pos,
            Some(&map_layout),
            map_settings.geometry,
            window_aspect_ratio(&windows),
            persp.fov,
            top_down_camera_yaw.0,
            client_settings.camera.top_down.margin,
            client_settings.camera.top_down.tilt_degrees,
        );
        return;
    }

    let config = client_settings.camera.follow;
    let eye_height = gameplay_config.player.eye_height();
    let rotation = Quat::from_euler(
        EulerRot::YXZ,
        local_player_info.stored_yaw,
        local_player_info.stored_pitch,
        0.0,
    );
    if third.distance > config.first_person_distance {
        let pivot = follow_pivot(player_pos, eye_height, config.pivot_height, third.pivot_blend());
        let radius = near_plane_radius(
            persp.near,
            persp.fov,
            window_aspect_ratio(&windows),
            config.collision_radius,
        );
        *camera_transform = third_person_transform(
            &collision_world,
            pivot,
            rotation,
            config,
            radius,
            time.delta_secs(),
            &mut third,
        );
    } else {
        third.arm_distance = 0.0;
        third.previous_pivot = None;
    }

    // Only the explicit zoom-in (`FollowCamera::zoom`) locks the facing; an
    // obstruction that collapses the arm is temporary and must not undo an
    // orbit the player unlocked.
    if third.arm_distance > config.first_person_distance {
        view_mode.set_if_neq(CameraViewMode::ThirdPerson);
    } else {
        view_mode.set_if_neq(CameraViewMode::FirstPerson);
        camera_transform.rotation = rotation;
        camera_transform.translation = eye_position(*player_pos, eye_height);
    }
    if let Some(shake) = maybe_shake {
        camera_transform.translation += Vec3::new(shake.offset_x, shake.offset_y, shake.offset_z);
    }
}

// The follow pivot rises from the eye toward the configured pivot height as
// the shoulder view eases in.
fn follow_pivot(feet: &Position, eye_height: f32, pivot_height: f32, blend: f32) -> Vec3 {
    Vec3::new(
        feet.x,
        feet.y + eye_height + (pivot_height - eye_height) * blend,
        feet.z,
    )
}

// The arm sweep must keep the whole near plane out of geometry, so its
// radius is at least the near plane's corner distance.
fn near_plane_radius(near: f32, fov: f32, aspect_ratio: f32, collision_radius: f32) -> f32 {
    let near_half_height = near * (fov * 0.5).tan();
    collision_radius.max(Vec3::new(near_half_height * aspect_ratio, near_half_height, near).length())
}

#[cfg(test)]
#[path = "tests/follow.rs"]
mod tests;
