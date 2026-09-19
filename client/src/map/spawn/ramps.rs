use bevy::prelude::*;

use super::{
    geometry_batch::{MapGeometryBatch, MapGeometryKind, SegmentTarget},
    ramp_mesh::build_ramp_meshes,
};
use crate::{carriers::CarrierStoreys, config::MapMaterials};
use common::protocol::*;

// Batches a ramp's faces, each under the material of the face it is: the
// slope is a top, a wedge's base or a plank's sloped underside a bottom, and
// every vertical face its cardinal.
pub fn batch_ramp(
    batcher: &mut MapGeometryBatch,
    map_materials: MapMaterials<'_>,
    storeys: &CarrierStoreys,
    ramp: &Ramp,
    material_ids: &FaceMaterials,
) {
    batcher.begin_segment(SegmentTarget {
        kind: MapGeometryKind::Ramp,
        carrier: ramp.carrier,
        level: storeys.tag(ramp.carrier, ramp.level, ramp.levels),
    });
    for (material_id, mesh) in build_ramp_meshes(ramp, material_ids, |alias| map_materials.get(alias).tile_size()) {
        batcher.add_mesh(material_id, &mesh, Transform::default());
    }
}
