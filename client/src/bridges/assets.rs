use bevy::prelude::*;

use crate::{
    constants::{BRIDGE_ALPHA_OFF, BRIDGE_EMISSIVE},
    vfx::{srgb_color, translucent_kind_material, with_white_vertex_colors},
};
use common::protocol::{BridgeKindId, KindDef};

// Sharing each kind's material keeps every bridge of that kind fading together.
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
    kinds: &[KindDef],
) -> BridgeAssets {
    let mesh = meshes.add(with_white_vertex_colors(
        Plane3d::default().mesh().size(1.0, 1.0).build(),
    ));

    let mut handles = Vec::with_capacity(kinds.len());
    let mut base_colors = Vec::with_capacity(kinds.len());
    for kind in kinds {
        let color = srgb_color(kind.color);
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

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::mesh::Indices;
    use common::protocol::HexColor;

    #[test]
    fn bridges_are_horizontal_double_sided_quads() {
        let mut meshes = Assets::default();
        let mut materials = Assets::default();
        let kinds = [KindDef {
            id: "blue".into(),
            color: HexColor([0, 0, 255]),
        }];
        let assets = build_bridge_assets(&mut meshes, &mut materials, &kinds);
        let mesh = meshes.get(&assets.mesh).expect("bridge mesh missing");
        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(|a| a.as_float3())
            .expect("bridge mesh positions missing");
        assert_eq!(positions.len(), 4);
        assert!(
            positions
                .iter()
                .all(|p| p[0].abs() == 0.5 && p[1] == 0.0 && p[2].abs() == 0.5)
        );
        assert_eq!(mesh.indices().map(Indices::len), Some(6));
        assert!(mesh.contains_attribute(Mesh::ATTRIBUTE_COLOR));

        let material = materials
            .get(assets.material_for(BridgeKindId(0)))
            .expect("bridge material missing");
        assert!(material.double_sided);
        assert_eq!(material.cull_mode, None);
        assert_eq!(material.alpha_mode, AlphaMode::Blend);
        assert_eq!(material.base_color.alpha(), BRIDGE_ALPHA_OFF);
    }
}
