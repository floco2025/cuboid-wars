use bevy::prelude::*;

use crate::constants::ITEM_SPIN_HZ;

const TAU: f32 = std::f32::consts::TAU;

// Per-entity spin phase in radians. Random initial value at spawn so nearby
// spinners don't move in lockstep. Wrapped to [0, TAU) every tick.
#[derive(Component)]
pub struct YSpinTimer(pub f32);

// Optional fixed orientation that the spin composes on top of. Use it when
// the mesh needs a specific "up" (e.g. apex-up tetrahedron, tilted capsule);
// omit it when the mesh is rotationally symmetric and the raw spin works.
#[derive(Component)]
pub struct YSpinBase(pub Quat);

// Slow Y-axis spin shared by keys and power-ups. When `YSpinBase` is present
// the final rotation is `Quat::from_rotation_y(timer) * base`, so the chosen
// up direction stays constant while the mesh sweeps around Y.
pub fn y_spin_system(time: Res<Time>, mut query: Query<(&mut Transform, &mut YSpinTimer, Option<&YSpinBase>)>) {
    let step = time.delta_secs() * ITEM_SPIN_HZ * TAU;
    for (mut transform, mut timer, base) in &mut query {
        timer.0 = (timer.0 + step).rem_euclid(TAU);
        let spin = Quat::from_rotation_y(timer.0);
        transform.rotation = base.map_or(spin, |b| spin * b.0);
    }
}
