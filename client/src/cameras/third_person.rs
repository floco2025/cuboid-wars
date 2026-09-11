use bevy::prelude::*;

use super::FollowCamera;
use crate::config::FollowCameraConfig;
use common::physics::CollisionWorld;

pub(super) fn third_person_transform(
    world: &CollisionWorld,
    pivot: Vec3,
    rotation: Quat,
    config: FollowCameraConfig,
    radius: f32,
    dt: f32,
    state: &mut FollowCamera,
) -> Transform {
    let distance = state.distance.clamp(0.0, config.max_distance);
    let offset = rotation * Vec3::new(state.shoulder_offset(config), 0.0, distance);
    let allowed = world.camera_arm_distance(pivot, offset, radius);
    let reset = state
        .previous_pivot
        .is_none_or(|previous| previous.distance(pivot) > config.max_distance);
    if reset || allowed < state.arm_distance {
        state.arm_distance = allowed;
    } else {
        state
            .arm_distance
            .smooth_nudge(&allowed, config.obstruction_return_rate, dt);
    }
    state.previous_pivot = Some(pivot);
    Transform {
        translation: pivot + offset.normalize_or_zero() * state.arm_distance,
        rotation,
        ..default()
    }
}

#[cfg(test)]
#[path = "tests/third_person.rs"]
mod tests;
