use bevy::prelude::*;

use crate::{constants::KEY_ROTATION_HZ, items::KeyRotationTimer, items::KeyMarker};

// Slow Y-axis spin for key entities, like a coin floating above the floor.
// Per-instance `KeyRotationTimer` carries a random phase so multiple keys
// near each other don't rotate in lockstep.
pub fn keys_rotate_system(time: Res<Time>, mut keys: Query<(&mut Transform, &mut KeyRotationTimer), With<KeyMarker>>) {
    let delta = time.delta_secs();
    for (mut transform, mut timer) in &mut keys {
        timer.0 += delta * KEY_ROTATION_HZ * std::f32::consts::TAU;
        transform.rotation = Quat::from_rotation_y(timer.0);
    }
}
