use crate::constants::{BARRIER_ALPHA_MAX, BARRIER_ALPHA_MIN, BARRIER_PULSE_HZ};
use bevy::prelude::*;
use std::f32::consts::TAU;

use super::BarrierAssets;
use crate::{config::ClientSettings, vfx::color_with_alpha};

// Drive each kind's shared material by a sine wave on `base_color.alpha`;
// the emissive is set once on the material and never pulsed, so the pulse
// reads as a translucency fade in / out. Per-kind phase offsets keep
// adjacent colors out of lockstep.
//
// Because each material handle is shared across every barrier of that kind
// one write here updates every visible instance — O(num_kinds) work per
// frame regardless of map size.
pub fn barriers_pulsate_system(
    time: Res<Time>,
    _client_settings: Res<ClientSettings>,
    barrier_assets: Res<BarrierAssets>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let assets = barrier_assets;
    let pulse_hz = BARRIER_PULSE_HZ;
    let alpha_min = BARRIER_ALPHA_MIN;
    let alpha_max = BARRIER_ALPHA_MAX;
    let t = time.elapsed_secs();
    for (idx, handle) in assets.material_handles().iter().enumerate() {
        let Some(mut mat) = materials.get_mut(handle) else {
            continue;
        };
        let phase = idx as f32 * 0.5;
        let s = (t * pulse_hz * TAU + phase).sin() * 0.5 + 0.5;
        let alpha = alpha_min + (alpha_max - alpha_min) * s;
        mat.base_color = color_with_alpha(assets.base_colors[idx], alpha);
    }
}
