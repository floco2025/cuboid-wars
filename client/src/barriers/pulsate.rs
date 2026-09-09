use std::f32::consts::TAU;

use bevy::prelude::*;

use super::BarrierAssets;
use crate::{
    config::{BarrierVfxConfig, ClientSettings},
    vfx::color_with_alpha,
};

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
    client_settings: Res<ClientSettings>,
    barrier_assets: Res<BarrierAssets>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let config = client_settings.vfx.barriers;
    let t = time.elapsed_secs();
    for (idx, kind) in barrier_assets.kinds.iter().enumerate() {
        let phase = idx as f32 * 0.5;
        let alpha = pulse_opacity(config, t, phase);
        // A `get_mut` marks the material modified and re-uploads it, so a
        // disabled pulse must not touch it every frame.
        if materials
            .get(&kind.surface)
            .is_none_or(|mat| mat.base_color.alpha() == alpha)
        {
            continue;
        }
        if let Some(mut mat) = materials.get_mut(&kind.surface) {
            mat.base_color = color_with_alpha(kind.base_color, alpha);
        }
    }
}

fn pulse_opacity(config: BarrierVfxConfig, time: f32, phase: f32) -> f32 {
    let pulse = config.pulse;
    if pulse.frequency_hz == 0.0 {
        return config.opacity;
    }
    let s = (time * pulse.frequency_hz * TAU + phase).sin() * 0.5 + 0.5;
    pulse.min_opacity + (config.opacity - pulse.min_opacity) * s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::BarrierPulseVfxConfig;

    #[test]
    fn pulse_uses_configured_range_and_frequency_and_can_be_disabled() {
        let mut config = BarrierVfxConfig {
            emissive_brightness: 2.0,
            opacity: 0.6,
            pulse: BarrierPulseVfxConfig {
                min_opacity: 0.2,
                frequency_hz: 2.0,
            },
        };
        assert!((pulse_opacity(config, 0.125, 0.0) - 0.6).abs() < 1e-6);
        assert!((pulse_opacity(config, 0.375, 0.0) - 0.2).abs() < 1e-6);

        config.pulse.frequency_hz = 0.0;
        assert_eq!(pulse_opacity(config, 0.375, 0.0), config.opacity);
    }
}
