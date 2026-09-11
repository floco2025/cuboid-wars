use bevy::prelude::*;

use crate::{
    config::BarrierVfxConfig,
    constants::*,
    fields::KindVisual,
    items::{item_symbol_mesh, pickup_material},
    vfx::srgb_color,
};
use common::protocol::{BarrierId, BarrierKindId, ItemType, KindDef, MapLayout};

// Indexed by `BarrierKindId`, in the map's kind order.
#[derive(Resource)]
pub struct BarrierAssets {
    pub(super) kinds: Vec<KindVisual>,
    key_mesh: Handle<Mesh>,
    barrier_kinds: Vec<BarrierKindId>,
}

impl BarrierAssets {
    pub fn key_color(&self, kind: BarrierKindId) -> Color {
        self.base_color(kind)
    }

    pub fn field_color(&self, id: BarrierId) -> Color {
        self.base_color(self.barrier_kinds[id.0 as usize])
    }

    pub fn material_for(&self, kind: BarrierKindId) -> &Handle<StandardMaterial> {
        &self.kinds[kind.0 as usize].surface
    }

    // sRGB base color for the kind, useful for HUD icons that aren't 3D
    // materials.
    pub fn base_color(&self, kind: BarrierKindId) -> Color {
        self.kinds[kind.0 as usize].base_color
    }

    pub fn key_mesh(&self) -> &Handle<Mesh> {
        &self.key_mesh
    }

    pub fn key_material_for(&self, kind: BarrierKindId) -> &Handle<StandardMaterial> {
        self.kinds[usize::from(kind.0)]
            .key_material
            .as_ref()
            .expect("key material missing from barrier kind")
    }
}

pub fn build_barrier_assets(
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    kinds: &[KindDef],
    layout: &MapLayout,
    config: BarrierVfxConfig,
    pickup_glow: f32,
) -> BarrierAssets {
    let key_mesh = meshes.add(item_symbol_mesh(
        ItemType::Key(BarrierKindId(0)),
        ITEM_KEY_SIZE,
        ITEM_KEY_DEPTH,
    ));
    let kinds = kinds
        .iter()
        .map(|kind| {
            let color = srgb_color(kind.color);
            KindVisual {
                key_material: Some(materials.add(pickup_material(color, pickup_glow))),
                ..KindVisual::new(materials, color, config.opacity, config.emissive_brightness)
            }
        })
        .collect();

    BarrierAssets {
        kinds,
        key_mesh,
        barrier_kinds: layout.barriers.iter().map(|b| b.kind).collect(),
    }
}

#[cfg(test)]
#[path = "tests/assets.rs"]
mod tests;
