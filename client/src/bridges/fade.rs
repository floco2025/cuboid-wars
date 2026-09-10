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
    for (idx, visual) in bridge_assets.kinds.iter().enumerate() {
        let kind = BridgeKindId(u16::try_from(idx).expect("bridge kind index exceeds u16"));
        let Some(alpha) = materials
            .get(&visual.surface)
            .map(|material| material.base_color.alpha())
        else {
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
        if let Some(mut material) = materials.get_mut(&visual.surface) {
            material.base_color = color_with_alpha(visual.base_color, next);
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
#[path = "tests/fade.rs"]
mod tests;
