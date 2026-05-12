use bevy::prelude::*;

use super::BarrierAssets;
use crate::constants::*;

// Drive each kind's shared emissive intensity by a sine wave. Per-kind phase
// offsets (derived from the kind index) keep adjacent colors visually out of
// lockstep. Because each material handle is shared across every barrier of
// that kind (plus every same-kind key), one write here updates every visible
// instance — O(num_kinds) work per frame regardless of map size.
pub fn barriers_pulsate_system(
    time: Res<Time>,
    barrier_assets: Option<Res<BarrierAssets>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(assets) = barrier_assets else { return };
    let t = time.elapsed_secs();
    for (idx, handle) in assets.material_handles().iter().enumerate() {
        let Some(mat) = materials.get_mut(handle) else { continue };
        let phase = idx as f32 * 0.5;
        let s = (t * BARRIER_PULSE_HZ * std::f32::consts::TAU + phase).sin() * 0.5 + 0.5;
        let intensity = BARRIER_EMISSIVE_MIN + (BARRIER_EMISSIVE_MAX - BARRIER_EMISSIVE_MIN) * s;
        let base = assets.base_colors[idx];
        mat.emissive = base.to_linear() * intensity;
    }
}
