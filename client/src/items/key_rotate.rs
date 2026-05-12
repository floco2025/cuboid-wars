use bevy::prelude::*;

use crate::constants::KEY_ROTATION_HZ;

const TAU: f32 = std::f32::consts::TAU;

// Marker for client-spawned key entities. The rotation system queries this.
#[derive(Component)]
pub struct KeyMarker;

// Per-key rotation phase in radians. Independent random initial value per
// entity so multiple keys near each other don't rotate in lockstep.
// Wrapped to [0, TAU) every tick so the phase doesn't accumulate f32 error
// over long sessions.
#[derive(Component)]
pub struct KeyRotationTimer(pub f32);

// Slow Y-axis spin for key entities, like a coin floating above the floor.
pub fn keys_rotate_system(time: Res<Time>, mut keys: Query<(&mut Transform, &mut KeyRotationTimer), With<KeyMarker>>) {
    let delta = time.delta_secs();
    let step = delta * KEY_ROTATION_HZ * TAU;
    for (mut transform, mut timer) in &mut keys {
        timer.0 = (timer.0 + step).rem_euclid(TAU);
        transform.rotation = Quat::from_rotation_y(timer.0);
    }
}
