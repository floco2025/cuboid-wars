use bevy::prelude::*;

use super::BridgeAssets;
use crate::{
    config::{ClientSettings, LightBridgeVfxConfig},
    vfx::{color_with_alpha, ease_blend},
};
use common::protocol::{BridgeKindId, PlateState};

const BRIDGE_FADE_SNAP: f32 = 0.002;

// Ease each kind's shared material alpha toward its powered/unpowered level.
// One write per kind reaches every bridge of that kind; the material's own
// `base_color.alpha` is the fade state.
pub fn bridges_fade_system(
    time: Res<Time>,
    client_settings: Res<ClientSettings>,
    plates: Res<PlateState>,
    bridge_assets: Res<BridgeAssets>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let config = client_settings.vfx.light_bridges;
    for (idx, handle) in bridge_assets.material_handles().enumerate() {
        let kind = BridgeKindId(u16::try_from(idx).expect("bridge kind index exceeds u16"));
        let Some(alpha) = materials.get(handle).map(|material| material.base_color.alpha()) else {
            continue;
        };
        let Some(next) = fade_step(
            alpha,
            fade_target(&plates, kind, config),
            time.delta_secs(),
            config.fade_secs,
        ) else {
            continue;
        };
        // `get_mut` marks the asset modified and re-extracts it to the GPU,
        // so a settled kind is left untouched.
        if let Some(mut material) = materials.get_mut(handle) {
            material.base_color = color_with_alpha(bridge_assets.base_color(kind), next);
        }
    }
}

fn fade_target(plates: &PlateState, kind: BridgeKindId, config: LightBridgeVfxConfig) -> f32 {
    if plates.powered_bridge_kinds.contains(&kind) {
        config.opacity
    } else {
        config.unpowered_opacity
    }
}

// Frame-rate independent easing; `None` once settled on the target.
fn fade_step(alpha: f32, target: f32, delta_secs: f32, fade_secs: f32) -> Option<f32> {
    if (alpha - target).abs() <= f32::EPSILON {
        return None;
    }
    let next = alpha + (target - alpha) * ease_blend(delta_secs, fade_secs);
    Some(if (next - target).abs() < BRIDGE_FADE_SNAP {
        target
    } else {
        next
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fade_targets_follow_the_powered_kinds() {
        let plates = PlateState {
            open_barrier_kinds: Vec::new(),
            powered_bridge_kinds: vec![BridgeKindId(1)],
        };
        let config = LightBridgeVfxConfig {
            emissive_brightness: 1.0,
            opacity: 0.6,
            unpowered_opacity: 0.1,
            fade_secs: 0.25,
        };
        assert_eq!(fade_target(&plates, BridgeKindId(1), config), config.opacity);
        assert_eq!(fade_target(&plates, BridgeKindId(0), config), config.unpowered_opacity);
    }

    #[test]
    fn fade_step_approaches_and_settles_then_stops_writing() {
        let config = LightBridgeVfxConfig {
            emissive_brightness: 1.0,
            opacity: 0.8,
            unpowered_opacity: 0.15,
            fade_secs: 0.25,
        };
        let mut alpha = config.unpowered_opacity;
        let first = fade_step(alpha, config.opacity, 0.05, config.fade_secs).expect("first step reports settled");
        assert!(first > alpha && first < config.opacity);
        alpha = first;
        for _ in 0..200 {
            match fade_step(alpha, config.opacity, 0.05, config.fade_secs) {
                Some(next) => alpha = next,
                None => break,
            }
        }
        assert_eq!(alpha, config.opacity);
        assert_eq!(fade_step(alpha, config.opacity, 0.05, config.fade_secs), None);
    }

    #[test]
    fn fade_duration_controls_how_quickly_opacity_changes() {
        let fast = fade_step(0.1, 0.9, 0.1, 0.1).expect("fast fade reports settled");
        let slow = fade_step(0.1, 0.9, 0.1, 1.0).expect("slow fade reports settled");
        assert!(fast > slow);
        assert!((fast - (0.1 + 0.8 * (1.0 - (-1.0_f32).exp()))).abs() < 1e-6);
    }
}
