use bevy::prelude::*;

use super::{
    geometry_batch::{MapGeometryBatch, MapGeometryKind},
    helpers::build_ramp_meshes,
};
use crate::config::AssetSet;
use common::{constants::LEVEL_HEIGHT, protocol::*};

// Spawn a ramp entity based on shared `Ramp` config.
pub fn batch_ramp(batcher: &mut MapGeometryBatch, asset_set: &AssetSet, ramp: &Ramp) {
    let top_material_id = asset_set.material_ids_for_ramp_top(ramp).top;
    let side_material_ids = asset_set.material_ids_for_ramp_side(ramp);
    let side_material_id = side_material_ids.first().to_owned();
    let top_material_def = asset_set.material_by_id(&top_material_id);
    let side_material_def = asset_set.material_by_id(&side_material_id);

    // Build meshes split by material usage
    let (mesh_top, mesh_side) = build_ramp_meshes(
        ramp.x1,
        ramp.z1,
        ramp.x2,
        ramp.z2,
        ramp.y1,
        ramp.y2,
        top_material_def.tile_size(),
        side_material_def.tile_size(),
    );

    // Lower of the two levels this ramp connects (derived from the lower y).
    let y_low = ramp.y1.min(ramp.y2);
    let lower_level = (y_low / LEVEL_HEIGHT).round().clamp(0.0, f32::from(u8::MAX)) as u8;

    batcher.add_mesh(
        MapGeometryKind::Ramp,
        lower_level,
        top_material_id,
        &mesh_top,
        Transform::default(),
    );
    batcher.add_mesh(
        MapGeometryKind::Ramp,
        lower_level,
        side_material_id,
        &mesh_side,
        Transform::default(),
    );
}
