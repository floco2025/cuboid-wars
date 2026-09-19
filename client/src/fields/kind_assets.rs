use bevy::prelude::*;

use super::KindVisual;
use crate::{
    constants::{ITEM_KEY_DEPTH, ITEM_KEY_SIZE},
    items::item_symbol_mesh,
    vfx::srgb_color,
};
use common::protocol::{FieldId, FieldKindId, ItemType, KindDef, MapLayout};

// The look barriers, light bridges, and keys of one kind share, indexed by
// `FieldKindId` in the map's kind order, with each placed field's kind.
#[derive(Resource)]
pub struct FieldAssets {
    kinds: Vec<KindVisual>,
    key_mesh: Handle<Mesh>,
    barrier_kinds: Vec<FieldKindId>,
    bridge_kinds: Vec<FieldKindId>,
}

impl FieldAssets {
    pub fn field_color(&self, field: FieldId) -> Color {
        self.base_color(match field {
            FieldId::Barrier(id) => self.barrier_kinds[id.0 as usize],
            FieldId::Bridge(id) => self.bridge_kinds[id.0 as usize],
        })
    }

    pub(crate) fn kind(&self, kind: FieldKindId) -> &KindVisual {
        &self.kinds[usize::from(kind.0)]
    }

    // sRGB base color for the kind, useful for HUD icons that aren't 3D
    // materials.
    pub fn base_color(&self, kind: FieldKindId) -> Color {
        self.kind(kind).base_color
    }

    pub fn key_mesh(&self) -> &Handle<Mesh> {
        &self.key_mesh
    }

    pub fn key_material_for(&self, kind: FieldKindId) -> &Handle<StandardMaterial> {
        &self.kind(kind).key_material
    }
}

pub fn build_field_assets(
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    kinds: &[KindDef],
    layout: &MapLayout,
    rail_emissive: f32,
    pickup_glow: f32,
) -> FieldAssets {
    let key_mesh = meshes.add(item_symbol_mesh(
        ItemType::Key(FieldKindId(0)),
        ITEM_KEY_SIZE,
        ITEM_KEY_DEPTH,
    ));
    FieldAssets {
        kinds: kinds
            .iter()
            .map(|kind| KindVisual::new(materials, srgb_color(kind.color), rail_emissive, pickup_glow))
            .collect(),
        key_mesh,
        barrier_kinds: layout.barriers.iter().map(|barrier| barrier.kind).collect(),
        bridge_kinds: layout.light_bridges.iter().map(|bridge| bridge.kind).collect(),
    }
}

#[cfg(test)]
#[path = "tests/kind_assets.rs"]
mod tests;
