use bevy::prelude::*;
use common::protocol::Health;

use crate::{
    cameras::{MainCameraMarker, RearviewCameraMarker},
    constants::LABEL_CULL_DISTANCE,
    ui::floating_labels::{CharacterLabelMeshMarker, LabelCamera},
};

// Make floating character labels face the main camera while staying upright.
pub fn floating_labels_billboard_system(
    camera_query: Query<&GlobalTransform, (With<Camera3d>, Without<RearviewCameraMarker>)>,
    mut text_mesh_query: Query<(&GlobalTransform, &mut Transform), With<CharacterLabelMeshMarker>>,
) {
    let Ok(camera_transform) = camera_query.single() else {
        return;
    };

    let camera_pos = camera_transform.translation();

    for (global_transform, mut transform) in &mut text_mesh_query {
        let text_pos = global_transform.translation();
        let direction = Vec3::new(camera_pos.x - text_pos.x, 0.0, camera_pos.z - text_pos.z);
        if direction.length_squared() <= 0.0001 {
            continue;
        }

        let world_rotation = Quat::from_rotation_y(direction.x.atan2(direction.z));
        let global_rotation = global_transform.to_scale_rotation_translation().1;
        let global_y_angle = global_rotation.to_euler(EulerRot::YXZ).0;
        let local_y_angle = transform.rotation.to_euler(EulerRot::YXZ).0;
        let parent_y_angle = global_y_angle - local_y_angle;
        let world_y_angle = world_rotation.to_euler(EulerRot::YXZ).0;
        let new_local_y_angle = world_y_angle - parent_y_angle;
        transform.rotation = Quat::from_rotation_y(new_local_y_angle);
    }
}

// Toggle each character's label-render camera on or off based on:
//   1. Distance to the main camera (cull beyond LABEL_CULL_DISTANCE).
//   2. Whether the character's `Health` was written this frame.
//
// Combined: a label camera renders only on frames where the character is
// within range AND something about its health changed (which `sync_actors`
// and `sync_players` re-insert each server snapshot, so this fires at the
// snapshot rate, not the frame rate).
//
// First-render correctness: a freshly spawned character's camera defaults
// to `is_active = true`, so the initial render happens before this system
// runs. On the next tick `is_changed()` is true for the just-inserted
// component, so the camera renders again then gets disabled the frame after
// if no further changes - two renders at spawn, fine.
pub fn floating_label_camera_visibility_system(
    main_camera: Query<&GlobalTransform, With<MainCameraMarker>>,
    characters: Query<(&GlobalTransform, &LabelCamera, Ref<Health>)>,
    mut cameras: Query<&mut Camera>,
) {
    let Ok(main_xf) = main_camera.single() else {
        return;
    };
    let main_pos = main_xf.translation();
    let cull_sq = LABEL_CULL_DISTANCE * LABEL_CULL_DISTANCE;

    for (char_xf, label_cam, health) in &characters {
        let in_range = char_xf.translation().distance_squared(main_pos) <= cull_sq;
        let dirty = health.is_changed();
        if let Ok(mut cam) = cameras.get_mut(label_cam.0) {
            cam.is_active = in_range && dirty;
        }
    }
}
