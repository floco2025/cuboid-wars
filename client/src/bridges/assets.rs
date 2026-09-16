use bevy::prelude::*;

use crate::{fields::KindVisual, vfx::srgb_color};
use common::protocol::{BridgeId, BridgeKindId, KindDef, MapLayout};

// Indexed by `BridgeKindId`, in the map's kind order.
#[derive(Resource)]
pub struct BridgeAssets {
    kinds: Vec<KindVisual>,
    bridge_kinds: Vec<BridgeKindId>,
}

impl BridgeAssets {
    pub fn field_color(&self, id: BridgeId) -> Color {
        self.kind(self.bridge_kinds[id.0 as usize]).base_color
    }

    pub(super) fn kind(&self, kind: BridgeKindId) -> &KindVisual {
        &self.kinds[usize::from(kind.0)]
    }
}

pub fn build_bridge_assets(
    materials: &mut Assets<StandardMaterial>,
    kinds: &[KindDef],
    layout: &MapLayout,
    rail_emissive: f32,
) -> BridgeAssets {
    BridgeAssets {
        kinds: kinds
            .iter()
            .map(|kind| KindVisual::new(materials, srgb_color(kind.color), rail_emissive))
            .collect(),
        bridge_kinds: layout.light_bridges.iter().map(|bridge| bridge.kind).collect(),
    }
}

#[cfg(test)]
#[path = "tests/assets.rs"]
mod tests;
