use bevy::prelude::*;

use crate::constants::*;
use crate::spawning::ItemAnimTimer;

// ============================================================================
// Items Animation System
// ============================================================================

// Animate items bobbing up and down
pub fn items_animation_system(time: Res<Time>, mut query: Query<(&mut Transform, &mut ItemAnimTimer)>) {
    let delta = time.delta_secs();

    for (mut transform, mut timer) in &mut query {
        timer.0 += delta * ITEM_ANIMATION_SPEED; // Speed of animation
        let offset = (timer.0 * std::f32::consts::TAU).sin() * ITEM_ANIMATION_HEIGHT; // Bob up/down by 10cm
        transform.translation.y = ITEM_HEIGHT_ABOVE_FLOOR + ITEM_SIZE / 2.0 + offset;
    }
}
