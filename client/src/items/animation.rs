use bevy::prelude::*;

use crate::{constants::*, items::ItemAnimTimer};
use common::protocol::{ItemMarker, Position};

// ============================================================================
// Items Animation System
// ============================================================================

// Animate items bobbing up and down
pub fn items_animation_system(
    time: Res<Time>,
    mut query: Query<(&Position, &mut Transform, &mut ItemAnimTimer), With<ItemMarker>>,
) {
    let delta = time.delta_secs();

    for (position, mut transform, mut timer) in &mut query {
        timer.0 += delta * ITEM_ANIMATION_SPEED;
        let offset = (timer.0 * std::f32::consts::TAU).sin() * ITEM_ANIMATION_HEIGHT;
        transform.translation.y = position.y + ITEM_HEIGHT_ABOVE_FLOOR + ITEM_SIZE / 2.0 + offset;
    }
}
