use bevy::prelude::*;

use super::BridgeAssets;
use crate::{
    config::{ClientSettings, LightBridgeVfxConfig},
    vfx::color_with_alpha,
};
use common::protocol::{BridgeId, PlateState};

const BRIDGE_FADE_SNAP: f32 = 0.002;

pub fn bridges_fade_system(
    time: Res<Time>,
    client_settings: Res<ClientSettings>,
    plates: Res<PlateState>,
    bridge_assets: Res<BridgeAssets>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let config = client_settings.vfx.light_bridges;
    for (bridge, visual) in &bridge_assets.visuals {
        let Some(alpha) = materials
            .get(&visual.surface)
            .map(|material| material.base_color.alpha())
        else {
            continue;
        };
        let Some(next) = fade_step(
            alpha,
            fade_target(&plates, *bridge, config),
            time.delta_secs(),
            config.fade_secs,
        ) else {
            continue;
        };
        // `get_mut` marks the asset modified and re-extracts it to the GPU,
        // so a settled bridge is left untouched.
        if let Some(mut material) = materials.get_mut(&visual.surface) {
            material.base_color = color_with_alpha(visual.base_color, next);
        }
    }
}

fn fade_target(plates: &PlateState, bridge: BridgeId, config: LightBridgeVfxConfig) -> f32 {
    if plates.powered_bridges.contains(&bridge) {
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
    let mut next = alpha;
    next.smooth_nudge(&target, 1.0 / fade_secs, delta_secs);
    Some(if (next - target).abs() < BRIDGE_FADE_SNAP {
        target
    } else {
        next
    })
}

#[cfg(test)]
#[path = "tests/fade.rs"]
mod tests;
