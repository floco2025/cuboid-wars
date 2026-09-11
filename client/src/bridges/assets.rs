use bevy::prelude::*;

use crate::{config::LightBridgeVfxConfig, fields::KindVisual, vfx::srgb_color};
use common::protocol::{BridgeId, KindDef, MapLayout};

// Separate instance materials let same-color bridges respond independently.
#[derive(Resource)]
pub struct BridgeAssets {
    pub(super) bridges: Vec<KindVisual>,
}

impl BridgeAssets {
    pub fn field_color(&self, id: BridgeId) -> Color {
        self.bridges[id.0 as usize].base_color
    }
}

pub fn build_bridge_assets(
    materials: &mut Assets<StandardMaterial>,
    kinds: &[KindDef],
    layout: &MapLayout,
    config: LightBridgeVfxConfig,
) -> BridgeAssets {
    let bridges = layout
        .light_bridges
        .iter()
        .map(|bridge| {
            KindVisual::new(
                materials,
                srgb_color(kinds[usize::from(bridge.kind.0)].color),
                config.unpowered_opacity,
                config.emissive_brightness,
            )
        })
        .collect();
    BridgeAssets { bridges }
}

#[cfg(test)]
#[path = "tests/assets.rs"]
mod tests;
