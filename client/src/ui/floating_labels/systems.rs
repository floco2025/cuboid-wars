use bevy::prelude::*;
use common::{health::health_ratio, protocol::Health};

use crate::{
    cameras::RearviewCameraMarker,
    ui::floating_labels::{
        LabelCamera,
        spawn::{CharacterLabelMeshMarker, FloatingHealthBarFill, floating_health_bar_fill_transform},
    },
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

// Render each player's name label texture for the few frames after spawn
// (`render_ttl` primed at spawn), then idle forever. The name never changes,
// so a one-time render is enough; the multi-frame window (not a single frame)
// makes that render land reliably regardless of frame timing. Health bars are
// separate geometry now, so this no longer reacts to health at all.
pub fn player_name_label_render_system(mut characters: Query<&mut LabelCamera>, mut cameras: Query<&mut Camera>) {
    for mut label_cam in &mut characters {
        let active = label_cam.render_ttl > 0;
        if let Ok(mut cam) = cameras.get_mut(label_cam.camera)
            && cam.is_active != active
        {
            cam.is_active = active;
        }
        if label_cam.render_ttl > 0 {
            label_cam.render_ttl -= 1;
        }
    }
}

// Rescale each character's world-space health-bar fill quad to its tracked
// character's current health. The bar is plain geometry rendered by the main
// camera (no render target), so this directly and reliably reflects every
// `Health` change. Writes only on change to avoid dirtying transforms.
pub fn floating_health_bar_fill_system(
    health_query: Query<&Health>,
    mut fill_query: Query<(&FloatingHealthBarFill, &mut Transform)>,
) {
    for (fill, mut transform) in &mut fill_query {
        let Ok(health) = health_query.get(fill.tracked_entity) else {
            continue;
        };
        let target = floating_health_bar_fill_transform(health_ratio(*health, fill.max_health), fill.full_width);
        if transform.translation.x != target.translation.x || transform.scale.x != target.scale.x {
            transform.translation = target.translation;
            transform.scale = target.scale;
        }
    }
}
