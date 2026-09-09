use bevy::prelude::*;
use std::f32::consts::TAU;

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
        let offset = (timer.0 * TAU).sin() * ITEM_ANIMATION_HEIGHT;
        transform.translation.y = position.y + ITEM_HEIGHT_ABOVE_FLOOR + offset;
    }
}

// Per-entity spin phase in radians. Random initial value at spawn so nearby
// spinners don't move in lockstep. Wrapped to [0, TAU) every tick.
#[derive(Component)]
pub struct YSpinTimer(pub f32);

// Fixed orientation that the spin composes on top of: the mesh's "up" (an
// upright portal ring, a tilted missile).
#[derive(Component)]
pub struct YSpinBase(pub Quat);

// Slow Y-axis spin shared by all pickups. The final rotation is
// `Quat::from_rotation_y(timer) * base`, so the chosen up direction stays
// constant while the mesh sweeps around Y.
pub fn y_spin_system(time: Res<Time>, mut query: Query<(&mut Transform, &mut YSpinTimer, &YSpinBase)>) {
    let step = time.delta_secs() * ITEM_SPIN_HZ * TAU;
    for (mut transform, mut timer, base) in &mut query {
        timer.0 = (timer.0 + step).rem_euclid(TAU);
        transform.rotation = Quat::from_rotation_y(timer.0) * base.0;
    }
}
