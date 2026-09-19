use bevy::prelude::*;

use super::FieldVisual;
use crate::{
    constants::{ITEM_KEY_DEPTH, ITEM_KEY_SIZE},
    items::item_symbol_mesh,
    vfx::srgb_color,
};
use common::protocol::{FieldDef, FieldId, ItemType};

// The look the barriers, light bridges, and key of one field share, indexed
// by `FieldId` in the map's field order.
#[derive(Resource)]
pub struct FieldAssets {
    visuals: Vec<FieldVisual>,
    key_mesh: Handle<Mesh>,
}

impl FieldAssets {
    pub(crate) fn visual(&self, field: FieldId) -> &FieldVisual {
        &self.visuals[usize::from(field.0)]
    }

    // sRGB base color for the field, useful for HUD icons that aren't 3D
    // materials.
    pub fn base_color(&self, field: FieldId) -> Color {
        self.visual(field).base_color
    }

    pub fn key_mesh(&self) -> &Handle<Mesh> {
        &self.key_mesh
    }

    pub fn key_material_for(&self, field: FieldId) -> &Handle<StandardMaterial> {
        &self.visual(field).key_material
    }
}

pub fn build_field_assets(
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    fields: &[FieldDef],
    rail_emissive: f32,
    pickup_glow: f32,
) -> FieldAssets {
    let key_mesh = meshes.add(item_symbol_mesh(
        ItemType::Key(FieldId(0)),
        ITEM_KEY_SIZE,
        ITEM_KEY_DEPTH,
    ));
    FieldAssets {
        visuals: fields
            .iter()
            .map(|field| FieldVisual::new(materials, srgb_color(field.color), rail_emissive, pickup_glow))
            .collect(),
        key_mesh,
    }
}

#[cfg(test)]
#[path = "tests/field_assets.rs"]
mod tests;
