use bevy::prelude::*;

use super::{helpers::build_ramp_meshes, materials::MapMaterialCache};
use crate::config::AssetSet;
use crate::markers::*;
use common::{constants::LEVEL_HEIGHT, protocol::*};

#[derive(Bundle)]
struct RampBundle {
    mesh: Mesh3d,
    material: MeshMaterial3d<StandardMaterial>,
    transform: Transform,
    visibility: Visibility,
    marker: RampMarker,
}

// Spawn a ramp entity based on shared `Ramp` config.
pub fn spawn_ramp(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    material_cache: &mut MapMaterialCache,
    asset_server: &Res<AssetServer>,
    asset_set: &AssetSet,
    ramp: &Ramp,
) {
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

    let top_material = material_cache.standard(&top_material_id, top_material_def, asset_server, materials);
    let side_material = material_cache.standard(&side_material_id, side_material_def, asset_server, materials);

    // Lower of the two levels this ramp connects (derived from the lower y).
    let y_low = ramp.y1.min(ramp.y2);
    let lower_level = (y_low / LEVEL_HEIGHT).round().clamp(0.0, f32::from(u8::MAX)) as u8;

    // Top entity (floor texture)
    commands.spawn((
        RampBundle {
            mesh: Mesh3d(meshes.add(mesh_top)),
            material: MeshMaterial3d(top_material),
            transform: Transform::default(),
            visibility: Visibility::Visible,
            marker: RampMarker,
        },
        MapLevel(lower_level),
    ));

    // Side entity (wall texture)
    commands.spawn((
        RampBundle {
            mesh: Mesh3d(meshes.add(mesh_side)),
            material: MeshMaterial3d(side_material),
            transform: Transform::default(),
            visibility: Visibility::Visible,
            marker: RampMarker,
        },
        MapLevel(lower_level),
    ));
}
