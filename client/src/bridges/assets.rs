use bevy::prelude::*;

use crate::{
    config::{AssetSet, assets::parse_hex_color},
    constants::{BRIDGE_ALPHA_OFF, BRIDGE_EMISSIVE},
    vfx::{translucent_kind_material, with_white_vertex_colors},
};
use common::{
    constants::BRIDGE_THICKNESS,
    protocol::{BridgeKindId, BridgeKindTable},
};

// One unit slab for every bridge (per-instance `Transform.scale` encodes the
// rectangle) and one material per kind, indexed by `BridgeKindId.0`, so the
// fade system's single write per kind reaches every bridge of that kind.
#[derive(Resource)]
pub struct BridgeAssets {
    pub(super) mesh: Handle<Mesh>,
    pub(super) materials: Vec<Handle<StandardMaterial>>,
    // sRGB colors as configured; the fade system rebuilds `base_color` from
    // these with the current alpha.
    pub(super) base_colors: Vec<Color>,
}

impl BridgeAssets {
    pub fn material_for(&self, kind: BridgeKindId) -> &Handle<StandardMaterial> {
        &self.materials[usize::from(kind.0)]
    }

    pub fn material_handles(&self) -> &[Handle<StandardMaterial>] {
        &self.materials
    }

    pub fn base_color(&self, kind: BridgeKindId) -> Color {
        self.base_colors[usize::from(kind.0)]
    }
}

pub fn build_bridge_assets(
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    kind_table: &BridgeKindTable,
    asset_set: &AssetSet,
) -> BridgeAssets {
    let mesh = meshes.add(with_white_vertex_colors(Cuboid::new(1.0, BRIDGE_THICKNESS, 1.0).into()));

    let mut handles = Vec::with_capacity(kind_table.len());
    let mut base_colors = Vec::with_capacity(kind_table.len());
    for id in kind_table.ids() {
        let hex = asset_set
            .bridge_kind_color_hex(id)
            .expect("bridge kind color missing from config");
        let color = parse_hex_color(hex).unwrap_or_else(|err| panic!("invalid color {hex:?} for kind {id:?}: {err}"));
        handles.push(materials.add(translucent_kind_material(color, BRIDGE_ALPHA_OFF, BRIDGE_EMISSIVE)));
        base_colors.push(color);
    }
    assert_eq!(handles.len(), base_colors.len());

    BridgeAssets {
        mesh,
        materials: handles,
        base_colors,
    }
}
