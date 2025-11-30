use bevy::prelude::*;

use crate::constants::{POWERUP_HEIGHT_ABOVE_FLOOR, POWERUP_SIZE};
use crate::spawning::PowerUpMarker;

// Animate powerups bobbing up and down
pub fn animate_powerups_system(time: Res<Time>, mut query: Query<(&mut Transform, &mut PowerUpMarker)>) {
    let delta = time.delta_secs();

    for (mut transform, mut marker) in &mut query {
        marker.anim_timer += delta * 1.0; // Speed of animation
        let offset = (marker.anim_timer * std::f32::consts::TAU).sin() * 0.1; // Bob up/down by 10cm
        transform.translation.y = POWERUP_HEIGHT_ABOVE_FLOOR + POWERUP_SIZE / 2.0 + offset;
    }
}
