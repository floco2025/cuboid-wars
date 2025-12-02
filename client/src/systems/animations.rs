use bevy::prelude::*;

use crate::constants::{ITEM_HEIGHT_ABOVE_FLOOR, ITEM_SIZE};
use crate::spawning::ItemAnimTimer;

// Animate items bobbing up and down
pub fn animate_items_system(time: Res<Time>, mut query: Query<(&mut Transform, &mut ItemAnimTimer)>) {
    let delta = time.delta_secs();

    for (mut transform, mut timer) in &mut query {
        timer.0 += delta * 1.0; // Speed of animation
        let offset = (timer.0 * std::f32::consts::TAU).sin() * 0.1; // Bob up/down by 10cm
        transform.translation.y = ITEM_HEIGHT_ABOVE_FLOOR + ITEM_SIZE / 2.0 + offset;
    }
}
