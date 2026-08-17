use crate::constants::{BARRIER_ALPHA_MAX, BARRIER_ALPHA_MIN, BARRIER_PULSE_HZ};
use bevy::prelude::*;

use super::BarrierAssets;
use crate::config::ClientSettings;

// Drive each kind's shared material by a sine wave on `base_color.alpha`.
// With `unlit: true` + `AlphaMode::Blend`, the surface alpha is the only
// visual knob that actually responds (emissive is ignored when unlit). The
// pulse therefore reads as a translucency fade in / out. Per-kind phase
// offsets keep adjacent colors out of lockstep.
//
// Because each material handle is shared across every barrier of that kind
// (plus every same-kind key), one write here updates every visible instance
// — O(num_kinds) work per frame regardless of map size.
pub fn barriers_pulsate_system(
    time: Res<Time>,
    _client_settings: Res<ClientSettings>,
    barrier_assets: Option<Res<BarrierAssets>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(assets) = barrier_assets else { return };
    let pulse_hz = BARRIER_PULSE_HZ;
    let alpha_min = BARRIER_ALPHA_MIN;
    let alpha_max = BARRIER_ALPHA_MAX;
    let t = time.elapsed_secs();
    for (idx, handle) in assets.material_handles().iter().enumerate() {
        let Some(mut mat) = materials.get_mut(handle) else {
            continue;
        };
        let phase = idx as f32 * 0.5;
        let s = (t * pulse_hz * std::f32::consts::TAU + phase).sin() * 0.5 + 0.5;
        let alpha = alpha_min + (alpha_max - alpha_min) * s;
        let linear = assets.base_colors[idx].to_linear();
        mat.base_color = Color::srgba(linear.red, linear.green, linear.blue, alpha);
    }
}
