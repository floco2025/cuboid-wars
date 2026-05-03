use bevy::prelude::*;
use common::{
    markers::{ActorMarker, PlayerMarker},
    math::angle_delta_radians,
    protocol::FaceDirection,
};

use super::components::CharacterVisualTurnState;
use crate::{
    constants::{
        CHARACTER_VISUAL_TURN_MAX_ANGLE, CHARACTER_VISUAL_TURN_MAX_DURATION, CHARACTER_VISUAL_TURN_MIN_DURATION,
    },
    markers::{CharacterLabelMeshMarker, RearviewCameraMarker},
};

const VISUAL_TURN_RETARGET_THRESHOLD: f32 = 0.001; // radians

// Smooth rendered character rotation toward the gameplay face direction.
// FaceDirection itself stays immediate because shooting and networking use it.
pub fn characters_visual_turn_system(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<
        (
            Entity,
            &FaceDirection,
            &mut Transform,
            Option<&mut CharacterVisualTurnState>,
        ),
        (Or<(With<PlayerMarker>, With<ActorMarker>)>, Without<Camera3d>),
    >,
) {
    for (entity, face_dir, mut transform, maybe_turn) in &mut query {
        let current_yaw = transform.rotation.to_euler(EulerRot::YXZ).0;
        let Some(mut turn) = maybe_turn else {
            transform.rotation = Quat::from_rotation_y(face_dir.0);
            commands
                .entity(entity)
                .insert(CharacterVisualTurnState::settled(face_dir.0));
            continue;
        };

        let target_delta = angle_delta_radians(face_dir.0, turn.target_yaw);
        if target_delta.abs() > VISUAL_TURN_RETARGET_THRESHOLD {
            let turn_delta = angle_delta_radians(face_dir.0, current_yaw);
            turn.start_yaw = current_yaw;
            turn.target_yaw = current_yaw + turn_delta;
            turn.elapsed = 0.0;
            turn.duration = visual_turn_duration(turn_delta.abs());
        }

        if turn.elapsed >= turn.duration {
            transform.rotation = Quat::from_rotation_y(turn.target_yaw);
            continue;
        }

        turn.elapsed = (turn.elapsed + time.delta_secs()).min(turn.duration);
        let t = turn.elapsed / turn.duration;
        let visual_yaw = turn.start_yaw + angle_delta_radians(turn.target_yaw, turn.start_yaw) * t;
        transform.rotation = Quat::from_rotation_y(visual_yaw);
    }
}

fn visual_turn_duration(angle_radians: f32) -> f32 {
    let t = (angle_radians / CHARACTER_VISUAL_TURN_MAX_ANGLE.to_radians()).clamp(0.0, 1.0);
    CHARACTER_VISUAL_TURN_MIN_DURATION.lerp(CHARACTER_VISUAL_TURN_MAX_DURATION, t)
}

// Make floating character labels face the main camera while staying upright.
pub fn character_label_billboard_system(
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
