use bevy::prelude::*;

use super::surface::bridge_visuals;
use crate::{config::LightBridgeVfxConfig, fields::KindVisual, vfx::srgb_color};
use common::protocol::{BridgeId, KindDef, MapLayout};

// One material set per drawn surface (`bridge_visuals`), so same-color
// surfaces on different switches respond independently.
#[derive(Resource)]
pub struct BridgeAssets {
    pub(super) visuals: Vec<(BridgeId, KindVisual)>,
    // Each bridge's index into `visuals`.
    visual_of: Vec<usize>,
}

impl BridgeAssets {
    pub fn field_color(&self, id: BridgeId) -> Color {
        self.visual(id).base_color
    }

    pub(super) fn visual(&self, id: BridgeId) -> &KindVisual {
        &self.visuals[self.visual_of[id.0 as usize]].1
    }
}

pub fn build_bridge_assets(
    materials: &mut Assets<StandardMaterial>,
    kinds: &[KindDef],
    layout: &MapLayout,
    config: LightBridgeVfxConfig,
) -> BridgeAssets {
    let mut visual_of = vec![0; layout.light_bridges.len()];
    let visuals = bridge_visuals(layout)
        .iter()
        .enumerate()
        .map(|(index, visual)| {
            for member in &visual.members {
                visual_of[member.0 as usize] = index;
            }
            (
                visual.bridge.id,
                KindVisual::new(
                    materials,
                    srgb_color(kinds[usize::from(visual.bridge.kind.0)].color),
                    config.unpowered_opacity,
                    config.emissive_brightness,
                ),
            )
        })
        .collect();
    BridgeAssets { visuals, visual_of }
}

#[cfg(test)]
#[path = "tests/assets.rs"]
mod tests;
