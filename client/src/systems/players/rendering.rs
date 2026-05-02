use bevy::prelude::*;

use super::components::{CharacterVisualTurn, CuboidShake};
use crate::{
    constants::{
        CHARACTER_VISUAL_TURN_MAX_ANGLE, CHARACTER_VISUAL_TURN_MAX_DURATION, CHARACTER_VISUAL_TURN_MIN_DURATION,
    },
    markers::*,
};
use common::{
    config::GameplayConfig,
    markers::{ActorMarker, PlayerMarker},
    protocol::{FaceDirection, Position},
};

const VISUAL_TURN_RETARGET_THRESHOLD: f32 = 0.001; // radians

// ============================================================================
// Transform Sync Systems
// ============================================================================

// Update player Transform from Position component for rendering
pub fn players_transform_sync_system(
    gameplay_config: Res<GameplayConfig>,
    mut player_query: Query<(&Position, &mut Transform, Option<&CuboidShake>), With<PlayerMarker>>,
) {
    let player_physics = gameplay_config.characters.player.physics();
    for (pos, mut transform, maybe_shake) in &mut player_query {
        // Base position
        transform.translation.x = pos.x;
        transform.translation.y = player_physics.collider_center_y(pos.y);
        transform.translation.z = pos.z;

        // Apply shake offset if active
        if let Some(shake) = maybe_shake {
            transform.translation.x += shake.offset_x;
            transform.translation.z += shake.offset_z;
        }
    }
}

// Smooth the rendered character rotation toward the gameplay face direction.
// FaceDirection itself stays immediate because shooting and networking use it.
pub fn characters_face_to_transform_system(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<
        (Entity, &FaceDirection, &mut Transform, Option<&mut CharacterVisualTurn>),
        (Or<(With<PlayerMarker>, With<ActorMarker>)>, Without<Camera3d>),
    >,
) {
    for (entity, face_dir, mut transform, maybe_turn) in &mut query {
        let current_yaw = transform.rotation.to_euler(EulerRot::YXZ).0;
        let Some(mut turn) = maybe_turn else {
            transform.rotation = Quat::from_rotation_y(face_dir.0);
            commands.entity(entity).insert(CharacterVisualTurn::settled(face_dir.0));
            continue;
        };

        let target_delta = angle_delta(face_dir.0, turn.target_yaw);
        if target_delta.abs() > VISUAL_TURN_RETARGET_THRESHOLD {
            let turn_delta = angle_delta(face_dir.0, current_yaw);
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
        let visual_yaw = turn.start_yaw + angle_delta(turn.target_yaw, turn.start_yaw) * t;
        transform.rotation = Quat::from_rotation_y(visual_yaw);
    }
}

fn visual_turn_duration(angle_radians: f32) -> f32 {
    let t = (angle_radians / CHARACTER_VISUAL_TURN_MAX_ANGLE.to_radians()).clamp(0.0, 1.0);
    CHARACTER_VISUAL_TURN_MIN_DURATION.lerp(CHARACTER_VISUAL_TURN_MAX_DURATION, t)
}

fn angle_delta(a: f32, b: f32) -> f32 {
    (a - b + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

// ============================================================================
// Billboard System
// ============================================================================

// Make player ID text meshes billboard (always face camera)
pub fn players_billboard_system(
    camera_query: Query<&GlobalTransform, (With<Camera3d>, Without<RearviewCameraMarker>)>,
    mut text_mesh_query: Query<(&GlobalTransform, &mut Transform), With<PlayerIdTextMeshMarker>>,
) {
    let Ok(camera_transform) = camera_query.single() else {
        return;
    };

    let camera_pos = camera_transform.translation();

    for (global_transform, mut transform) in &mut text_mesh_query {
        let text_pos = global_transform.translation();
        // Calculate direction to camera on XZ plane only (keep Y upright)
        let direction = Vec3::new(camera_pos.x - text_pos.x, 0.0, camera_pos.z - text_pos.z);
        if direction.length_squared() > 0.0001 {
            // Calculate world rotation needed to face camera
            let world_rotation = Quat::from_rotation_y(direction.x.atan2(direction.z));

            // Get the combined parent rotation from global transform
            let global_rotation = global_transform.to_scale_rotation_translation().1;
            // Extract just the Y rotation from global
            let global_y_angle = global_rotation.to_euler(EulerRot::YXZ).0;
            // Calculate what the local Y rotation is currently
            let local_y_angle = transform.rotation.to_euler(EulerRot::YXZ).0;
            // Parent Y rotation is the difference
            let parent_y_angle = global_y_angle - local_y_angle;

            // Calculate new local rotation that compensates for parent
            let world_y_angle = world_rotation.to_euler(EulerRot::YXZ).0;
            let new_local_y_angle = world_y_angle - parent_y_angle;
            transform.rotation = Quat::from_rotation_y(new_local_y_angle);
        }
    }
}
