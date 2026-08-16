use bevy::prelude::*;
use std::time::Duration;

use super::components::{CameraShake, CuboidShake};
use crate::cameras::MainCameraMarker;
use common::protocol::PlayerMarker;

// ============================================================================
// Visual Effects Systems
// ============================================================================

// Apply camera shake effect - updates shake offset
pub fn local_player_camera_shake_system(
    mut commands: Commands,
    time: Res<Time>,
    mut camera_query: Query<(Entity, &mut CameraShake), With<MainCameraMarker>>,
) {
    for (entity, mut shake) in &mut camera_query {
        update_camera_shake(&mut commands, entity, time.delta(), &mut shake);
    }
}

// Apply cuboid shake effect - updates shake offset
pub fn local_player_cuboid_shake_system(
    mut commands: Commands,
    time: Res<Time>,
    mut cuboid_query: Query<(Entity, &mut CuboidShake), With<PlayerMarker>>,
) {
    for (entity, mut shake) in &mut cuboid_query {
        update_cuboid_shake(&mut commands, entity, time.delta(), &mut shake);
    }
}

// Oscillations over the whole shake window (radians of sine phase); the
// camera rings faster than the hit cuboid wobble.
const CAMERA_SHAKE_OSCILLATION: f32 = 30.0;
const CUBOID_SHAKE_OSCILLATION: f32 = 20.0;

// Shared envelope: a sine at `oscillation` phase-rate whose amplitude decays
// linearly to zero over the timer.
fn shake_wave(progress: f32, intensity: f32, oscillation: f32) -> f32 {
    intensity * (1.0 - progress) * (progress * oscillation).sin()
}

fn update_camera_shake(commands: &mut Commands, entity: Entity, delta: Duration, shake: &mut CameraShake) {
    shake.timer.tick(delta);
    if shake.timer.is_finished() {
        commands.entity(entity).remove::<CameraShake>();
        return;
    }

    let wave = shake_wave(shake.timer.fraction(), shake.intensity, CAMERA_SHAKE_OSCILLATION);
    shake.offset_x = shake.dir_x * wave;
    shake.offset_y = shake.dir_y * wave;
    shake.offset_z = shake.dir_z * wave;
}

fn update_cuboid_shake(commands: &mut Commands, entity: Entity, delta: Duration, shake: &mut CuboidShake) {
    shake.timer.tick(delta);
    if shake.timer.is_finished() {
        commands.entity(entity).remove::<CuboidShake>();
        return;
    }

    let wave = shake_wave(shake.timer.fraction(), shake.intensity, CUBOID_SHAKE_OSCILLATION);
    shake.offset_x = shake.dir_x * wave;
    shake.offset_z = shake.dir_z * wave;
}
