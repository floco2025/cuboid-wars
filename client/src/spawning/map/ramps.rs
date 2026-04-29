use bevy::prelude::*;

use super::helpers::build_ramp_meshes;
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
    asset_server: &Res<AssetServer>,
    asset_set: &AssetSet,
    ramp: &Ramp,
) {
    let top_material_def = asset_set.material_for_ramp_top(ramp);
    let side_material_def = asset_set.material_for_ramp_side(ramp);

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

    // Floor material for the ramp top
    let mut top_material = top_material_def.standard_material(asset_server);
    top_material.alpha_mode = AlphaMode::Opaque;
    top_material.base_color.set_alpha(1.0);

    // Wall material for the ramp sides
    let mut side_material = side_material_def.standard_material(asset_server);
    side_material.alpha_mode = AlphaMode::Opaque;
    side_material.base_color.set_alpha(1.0);

    // Lower of the two levels this ramp connects (derived from the lower y).
    let y_low = ramp.y1.min(ramp.y2);
    let lower_level = (y_low / LEVEL_HEIGHT).round().clamp(0.0, f32::from(u8::MAX)) as u8;

    // Top entity (floor texture)
    commands.spawn((
        RampBundle {
            mesh: Mesh3d(meshes.add(mesh_top)),
            material: MeshMaterial3d(materials.add(top_material)),
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
            material: MeshMaterial3d(materials.add(side_material)),
            transform: Transform::default(),
            visibility: Visibility::Visible,
            marker: RampMarker,
        },
        MapLevel(lower_level),
    ));
}
