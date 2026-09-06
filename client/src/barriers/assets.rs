use bevy::prelude::*;

use crate::{
    constants::*,
    items::{item_symbol_mesh, pickup_material},
    vfx::{srgb_color, translucent_kind_material, with_white_vertex_colors},
};
use common::protocol::{BarrierKindId, ItemType, KindDef};

#[derive(Resource)]
pub struct BarrierAssets {
    pub(super) mesh: Handle<Mesh>,
    pub(super) key_mesh: Handle<Mesh>,
    pub(super) materials: Vec<Handle<StandardMaterial>>,
    key_materials: Vec<Handle<StandardMaterial>>,
    // Mirror of the table at construction time, so the pulsate system can
    // re-derive the base color without re-reading the config every frame.
    pub(super) base_colors: Vec<Color>,
}

impl BarrierAssets {
    pub fn material_for(&self, kind: BarrierKindId) -> &Handle<StandardMaterial> {
        &self.materials[kind.0 as usize]
    }

    pub fn material_handles(&self) -> &[Handle<StandardMaterial>] {
        &self.materials
    }

    // sRGB base color for the kind, useful for HUD icons that aren't 3D
    // materials.
    pub fn base_color(&self, kind: BarrierKindId) -> Color {
        self.base_colors[kind.0 as usize]
    }

    pub fn key_mesh(&self) -> &Handle<Mesh> {
        &self.key_mesh
    }

    pub fn key_material_for(&self, kind: BarrierKindId) -> &Handle<StandardMaterial> {
        &self.key_materials[kind.0 as usize]
    }
}

pub fn build_barrier_assets(
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    kinds: &[KindDef],
    pickup_glow: f32,
) -> BarrierAssets {
    let mesh = meshes.add(with_white_vertex_colors(Rectangle::new(1.0, 1.0).into()));
    let key_mesh = meshes.add(item_symbol_mesh(ItemType::Key(BarrierKindId(0)), KEY_SIZE, KEY_DEPTH));

    let mut handles = Vec::with_capacity(kinds.len());
    let mut base_colors = Vec::with_capacity(kinds.len());
    let mut key_materials = Vec::with_capacity(kinds.len());
    for kind in kinds {
        let color = srgb_color(kind.color);
        handles.push(materials.add(translucent_kind_material(color, BARRIER_ALPHA_MAX, BARRIER_EMISSIVE)));
        key_materials.push(materials.add(pickup_material(color, pickup_glow)));
        base_colors.push(color);
    }

    // Both vectors are indexed by `BarrierKindId.0` — a length divergence
    // would mean a future contributor split the loops apart. Catch that here
    // instead of as an out-of-bounds panic at first lookup.
    assert_eq!(handles.len(), base_colors.len());

    BarrierAssets {
        key_mesh,
        mesh,
        materials: handles,
        key_materials,
        base_colors,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::mesh::Indices;
    use common::protocol::HexColor;

    #[test]
    fn barriers_are_double_sided_quads_and_keys_use_solid_glowing_symbols() {
        let mut meshes = Assets::default();
        let mut materials = Assets::default();
        let kinds = [KindDef {
            id: "red".into(),
            color: HexColor([255, 0, 0]),
        }];
        let assets = build_barrier_assets(&mut meshes, &mut materials, &kinds, 3.0);
        let mesh = meshes.get(&assets.mesh).expect("barrier mesh missing");
        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(|a| a.as_float3())
            .expect("barrier mesh positions missing");
        assert_eq!(positions.len(), 4);
        assert!(
            positions
                .iter()
                .all(|p| p[0].abs() == 0.5 && p[1].abs() == 0.5 && p[2] == 0.0)
        );
        assert_eq!(mesh.indices().map(Indices::len), Some(6));
        assert!(mesh.contains_attribute(Mesh::ATTRIBUTE_COLOR));

        let key_mesh = meshes.get(assets.key_mesh()).expect("key mesh missing");
        let positions = key_mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(|a| a.as_float3())
            .expect("key mesh positions missing");
        assert!(positions.iter().all(|p| {
            p[0].abs() <= KEY_SIZE / 2.0 && p[1].abs() <= KEY_SIZE / 2.0 && p[2].abs() == KEY_DEPTH / 2.0
        }));

        let key_material = materials
            .get(assets.key_material_for(BarrierKindId(0)))
            .expect("key material missing");
        assert_eq!(key_material.alpha_mode, AlphaMode::Opaque);
        assert_eq!(key_material.emissive, LinearRgba::rgb(3.0, 0.0, 0.0));

        let material = materials
            .get(assets.material_for(BarrierKindId(0)))
            .expect("barrier material missing");
        assert!(material.double_sided);
        assert_eq!(material.cull_mode, None);
        assert_eq!(material.alpha_mode, AlphaMode::Blend);
        assert_eq!(material.base_color.alpha(), BARRIER_ALPHA_MAX);
    }
}
