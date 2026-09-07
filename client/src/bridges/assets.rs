use bevy::prelude::*;

use crate::{
    constants::{BRIDGE_ALPHA_OFF, BRIDGE_EMISSIVE},
    map::{FieldMaterials, FieldMeshes},
    vfx::srgb_color,
};
use common::protocol::{BridgeKindId, KindDef};

// Sharing each kind's material keeps every bridge of that kind fading together.
#[derive(Resource)]
pub struct BridgeAssets {
    pub(super) meshes: FieldMeshes,
    pub(super) fields: Vec<FieldMaterials>,
    // sRGB colors as configured; the fade system rebuilds `base_color` from
    // these with the current alpha.
    pub(super) base_colors: Vec<Color>,
}

impl BridgeAssets {
    pub fn material_for(&self, kind: BridgeKindId) -> &Handle<StandardMaterial> {
        &self.fields[usize::from(kind.0)].surface
    }

    pub fn material_handles(&self) -> impl Iterator<Item = &Handle<StandardMaterial>> {
        self.fields.iter().map(|field| &field.surface)
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
    let meshes = FieldMeshes::new(meshes);

    let mut handles = Vec::with_capacity(kinds.len());
    let mut base_colors = Vec::with_capacity(kinds.len());
    for kind in kinds {
        let color = srgb_color(kind.color);
        handles.push(FieldMaterials::new(materials, color, BRIDGE_ALPHA_OFF, BRIDGE_EMISSIVE));
        base_colors.push(color);
    }
    assert_eq!(handles.len(), base_colors.len());

    BridgeAssets {
        meshes,
        fields: handles,
        base_colors,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::mesh::Indices;
    use common::protocol::HexColor;

    #[test]
    fn bridges_use_double_sided_quads_and_solid_frames() {
        let mut meshes = Assets::default();
        let mut materials = Assets::default();
        let kinds = [KindDef {
            id: "blue".into(),
            color: HexColor([0, 0, 255]),
        }];
        let assets = build_bridge_assets(&mut meshes, &mut materials, &kinds);
        let mesh = meshes.get(&assets.meshes.panel).expect("bridge mesh missing");
        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(|a| a.as_float3())
            .expect("bridge mesh positions missing");
        assert_eq!(positions.len(), 4);
        assert!(
            positions
                .iter()
                .all(|p| p[0].abs() == 0.5 && p[1].abs() == 0.5 && p[2] == 0.0)
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
        let frame = materials
            .get(&assets.fields[0].frame)
            .expect("bridge frame material missing");
        assert_eq!(frame.alpha_mode, AlphaMode::Opaque);
        assert!(frame.unlit);
        assert_eq!(frame.base_color, assets.base_color(BridgeKindId(0)));
    }
}
