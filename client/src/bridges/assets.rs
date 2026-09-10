use bevy::prelude::*;

use crate::{config::LightBridgeVfxConfig, fields::KindVisual, vfx::srgb_color};
use common::protocol::{BridgeKindId, KindDef};

// Sharing each kind's material keeps every bridge of that kind fading
// together. Indexed by `BridgeKindId`, in the map's kind order.
#[derive(Resource)]
pub struct BridgeAssets {
    pub(super) kinds: Vec<KindVisual>,
}

impl BridgeAssets {
    pub fn material_for(&self, kind: BridgeKindId) -> &Handle<StandardMaterial> {
        &self.kinds[usize::from(kind.0)].surface
    }

    pub fn base_color(&self, kind: BridgeKindId) -> Color {
        self.kinds[usize::from(kind.0)].base_color
    }
}

pub fn build_bridge_assets(
    materials: &mut Assets<StandardMaterial>,
    kinds: &[KindDef],
    config: LightBridgeVfxConfig,
) -> BridgeAssets {
    let kinds = kinds
        .iter()
        .map(|kind| {
            KindVisual::new(
                materials,
                srgb_color(kind.color),
                config.unpowered_opacity,
                config.emissive_brightness,
            )
        })
        .collect();
    BridgeAssets { kinds }
}

#[cfg(test)]
#[path = "tests/assets.rs"]
mod tests;
