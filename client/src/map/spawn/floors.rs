use bevy::prelude::*;

use super::{
    cuboid_mesh::{tiled_cuboid, tiled_floor_surface_meshes, tiled_floor_top_mesh},
    geometry_batch::{MapGeometryBatch, MapGeometryKind, SegmentTarget},
};
use crate::{carriers::CarrierStoreys, config::AssetSet};
use common::protocol::{FaceMaterials, *};

const FLOOR_CUT_EPSILON: f32 = 0.0001;

// Spawn a visual cuboid slab for a `Floor`. Ground-storey floors get the
// ground texture and a `GroundMarker`; higher storeys get the roof texture
// and a `RoofMarker` so the R key / top-down view can hide them. The slab is
// `floor.thickness` deep, centered just below `floor.y` so the standing
// surface is at `floor.y`. The center is the record's own, so the texture
// is projected in the carrier's frame and never swims across a slab that
// moves.
pub fn batch_floor(
    batcher: &mut MapGeometryBatch,
    asset_set: &AssetSet,
    storeys: &CarrierStoreys,
    floor: &Floor,
    material_ids: &FaceMaterials,
    terrain: &[TerrainCell],
    cell_size: f32,
) {
    let level = storeys.tag(floor.carrier, floor.level, 0);
    let center_x = f32::midpoint(floor.x1, floor.x2);
    let center_z = f32::midpoint(floor.z1, floor.z2);
    let center_y = floor.y - floor.thickness / 2.0;
    let size_x = (floor.x2 - floor.x1).abs();
    let size_z = (floor.z2 - floor.z1).abs();
    let carrier_center = Vec3::new(center_x, center_y, center_z);
    let transform = Transform::from_translation(carrier_center);
    let kind = if level.level == 0 {
        MapGeometryKind::Ground
    } else {
        MapGeometryKind::Roof
    };
    batcher.begin_segment(SegmentTarget {
        kind,
        carrier: floor.carrier,
        level,
    });

    let top_rectangles = standard_top_rectangles(floor, material_ids, terrain, cell_size);
    let cut_top = !(top_rectangles.len() == 1
        && top_rectangles[0]
            == [
                floor.x1.min(floor.x2),
                floor.z1.min(floor.z2),
                floor.x1.max(floor.x2),
                floor.z1.max(floor.z2),
            ]);

    if material_ids.is_uniform() && !cut_top {
        let material_def = asset_set.material_by_id(material_ids.primary());
        let mesh = tiled_cuboid(
            size_x,
            floor.thickness,
            size_z,
            material_def.tile_size(),
            carrier_center,
            Quat::IDENTITY,
        );
        batcher.add_mesh(material_ids.primary(), &mesh, transform);
        return;
    }

    let north_material_def = asset_set.material_by_id(&material_ids.north);
    let south_material_def = asset_set.material_by_id(&material_ids.south);
    let east_material_def = asset_set.material_by_id(&material_ids.east);
    let west_material_def = asset_set.material_by_id(&material_ids.west);
    let top_material_def = asset_set.material_by_id(&material_ids.top);
    let bottom_material_def = asset_set.material_by_id(&material_ids.bottom);
    let surface_meshes = tiled_floor_surface_meshes(
        size_x,
        floor.thickness,
        size_z,
        carrier_center,
        north_material_def.tile_size(),
        south_material_def.tile_size(),
        east_material_def.tile_size(),
        west_material_def.tile_size(),
        bottom_material_def.tile_size(),
    );

    batcher.add_mesh(&material_ids.north, &surface_meshes.north, transform);
    batcher.add_mesh(&material_ids.south, &surface_meshes.south, transform);
    batcher.add_mesh(&material_ids.east, &surface_meshes.east, transform);
    batcher.add_mesh(&material_ids.west, &surface_meshes.west, transform);
    batcher.add_mesh(&material_ids.bottom, &surface_meshes.down, transform);
    if !top_rectangles.is_empty() {
        let top = tiled_floor_top_mesh(&top_rectangles, floor.y, carrier_center, top_material_def.tile_size());
        batcher.add_mesh(&material_ids.top, &top, transform);
    }
}

// A terrain floor's procedural top is spawned from the final compiled floor
// footprint, including its perimeter extensions and corner fillers. Omitting
// the complete ordinary top here avoids leaving those narrow pieces behind as
// the textureless `terrain` alias. Debug rendering passes an empty terrain
// slice and therefore keeps the complete ordinary top for inspection.
fn standard_top_rectangles(
    floor: &Floor,
    material_ids: &FaceMaterials,
    terrain: &[TerrainCell],
    cell_size: f32,
) -> Vec<[f32; 4]> {
    if !terrain.is_empty() && material_ids.top == TERRAIN_MATERIAL {
        Vec::new()
    } else {
        top_rectangles_without_terrain(floor, terrain, cell_size)
    }
}

// Partition one merged floor top at terrain-cell edges, then retain only
// regions whose midpoint is not covered. This removes the old top geometry
// outright instead of relying on a depth offset between coplanar surfaces.
fn top_rectangles_without_terrain(floor: &Floor, terrain: &[TerrainCell], cell_size: f32) -> Vec<[f32; 4]> {
    let (x1, x2, z1, z2) = floor.bounds_xz();
    let half = cell_size * 0.5;
    let cuts = terrain
        .iter()
        .filter(|cell| {
            cell.carrier == floor.carrier && cell.level == floor.level && (cell.y - floor.y).abs() <= FLOOR_CUT_EPSILON
        })
        .filter_map(|cell| {
            let cut_x1 = (cell.x - half).max(x1);
            let cut_x2 = (cell.x + half).min(x2);
            let cut_z1 = (cell.z - half).max(z1);
            let cut_z2 = (cell.z + half).min(z2);
            (cut_x2 - cut_x1 > FLOOR_CUT_EPSILON && cut_z2 - cut_z1 > FLOOR_CUT_EPSILON)
                .then_some([cut_x1, cut_z1, cut_x2, cut_z2])
        })
        .collect::<Vec<_>>();
    if cuts.is_empty() {
        return vec![[x1, z1, x2, z2]];
    }

    let mut xs = vec![x1, x2];
    let mut zs = vec![z1, z2];
    for cut in &cuts {
        xs.extend([cut[0], cut[2]]);
        zs.extend([cut[1], cut[3]]);
    }
    sort_and_dedup(&mut xs);
    sort_and_dedup(&mut zs);

    let mut visible = Vec::new();
    for x in xs.windows(2) {
        for z in zs.windows(2) {
            let midpoint = Vec2::new(f32::midpoint(x[0], x[1]), f32::midpoint(z[0], z[1]));
            let covered = cuts.iter().any(|cut| {
                midpoint.x >= cut[0] && midpoint.x <= cut[2] && midpoint.y >= cut[1] && midpoint.y <= cut[3]
            });
            if !covered {
                visible.push([x[0], z[0], x[1], z[1]]);
            }
        }
    }
    visible
}

fn sort_and_dedup(values: &mut Vec<f32>) {
    values.sort_by(f32::total_cmp);
    values.dedup_by(|a, b| (*a - *b).abs() <= FLOOR_CUT_EPSILON);
}

#[cfg(test)]
#[path = "tests/floors.rs"]
mod tests;
