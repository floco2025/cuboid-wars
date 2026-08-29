use bevy::prelude::*;

use crate::{
    config::{AssetSet, ClientSettings, MaterialDef},
    map::{MapLevel, tiled_cuboid},
};
use common::{
    constants::{GRID_CELL_SIZE, LEVEL_HEIGHT},
    protocol::{MapLayout, PlatePurpose},
};

#[derive(Component)]
pub struct PressurePlateMarker;

// The plate's purpose, matched against the snapshot's locked purposes.
#[derive(Component)]
pub struct PlatePurposeMarker(pub PlatePurpose);

// Snapshot state: plate purposes still locked behind a quest. Their plates
// are hidden here and inert server-side.
#[derive(Resource, Default)]
pub struct LockedPlatePurposes(pub Vec<PlatePurpose>);

fn plate_visibility(purpose: PlatePurpose, locked: &[PlatePurpose]) -> Visibility {
    if locked.contains(&purpose) {
        Visibility::Hidden
    } else {
        Visibility::Visible
    }
}

// Runs after the spawn system so a re-spawn can't leave a locked plate
// visible for a frame. Level focus never touches plates, so writing both
// directions here races nothing.
pub fn pressure_plates_visibility_system(
    locked: Res<LockedPlatePurposes>,
    mut plates: Query<(&PlatePurposeMarker, &mut Visibility), With<PressurePlateMarker>>,
) {
    for (purpose, mut visibility) in &mut plates {
        visibility.set_if_neq(plate_visibility(purpose.0, &locked.0));
    }
}

// Plate footprint: inner 50% per side (≈25% by area). Slightly above the
// floor to avoid z-fighting with the floor slab beneath.
const PLATE_SIDE: f32 = GRID_CELL_SIZE * 0.5;
const PLATE_Y_OFFSET: f32 = 0.01;
// Housing: a frame slab spanning the footprint with the panel inset on top
// (materials from `assets.json::pressure_plate`) — a physical mechanism
// rather than a painted decal. Every plate looks the same; its purpose is
// not shown.
const PLATE_FRAME_HEIGHT: f32 = 0.05;
const PLATE_PANEL_SIDE: f32 = PLATE_SIDE * 0.7;
const PLATE_PANEL_HEIGHT: f32 = 0.05;
// The panel sinks this far into the frame so no gap can show at the seam.
const PLATE_PANEL_OVERLAP: f32 = 0.01;

// Shared across every plate. Pub only because it appears in the spawn
// system's `Local` parameter.
pub struct PlateAssets {
    frame_mesh: Handle<Mesh>,
    panel_mesh: Handle<Mesh>,
    frame_material: Handle<StandardMaterial>,
    panel_material: Handle<StandardMaterial>,
}

impl PlateAssets {
    fn new(
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<StandardMaterial>,
        asset_server: &AssetServer,
        asset_set: &AssetSet,
        client_settings: &ClientSettings,
    ) -> Self {
        let rendering = &client_settings.rendering;
        let frame = asset_set.plate_frame_material_def();
        let panel = asset_set.plate_panel_material_def();
        let mut material = |def: &MaterialDef| {
            materials.add(def.standard_material(asset_server, rendering.texture_anisotropy, rendering.mipmaps))
        };
        Self {
            frame_mesh: meshes.add(plate_box(PLATE_SIDE, PLATE_FRAME_HEIGHT, frame.tile_size())),
            panel_mesh: meshes.add(plate_box(PLATE_PANEL_SIDE, PLATE_PANEL_HEIGHT, panel.tile_size())),
            frame_material: material(frame),
            panel_material: material(panel),
        }
    }
}

// UVs are mesh-local rather than world-anchored: both materials are uniform
// patterns and no neighbour shares them, so nothing needs to tile across a
// seam — and one mesh can serve every plate. Both materials carry normal
// maps, which only render on meshes with tangents.
fn plate_box(side: f32, height: f32, tile_size: f32) -> Mesh {
    let mut mesh = tiled_cuboid(side, height, side, tile_size, Vec3::ZERO, Quat::IDENTITY);
    mesh.generate_tangents()
        .expect("plate box mesh lacks the positions, normals, or UVs tangents need");
    mesh
}

// Spawn one housing per pressure plate in the current `MapLayout`. Re-runs
// when the layout is inserted or replaced (e.g. reconnect / map change).
// Plates are static — no animation; the puzzle feedback is the barrier
// disappearing when the threshold is met.
pub fn pressure_plates_spawn_system(
    mut commands: Commands,
    map_layout: Option<Res<MapLayout>>,
    asset_set: Res<AssetSet>,
    asset_server: Res<AssetServer>,
    client_settings: Res<ClientSettings>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut plate_assets: Local<Option<PlateAssets>>,
    existing: Query<Entity, With<PressurePlateMarker>>,
    locked: Res<LockedPlatePurposes>,
) {
    let Some(layout) = map_layout else { return };
    if !layout.is_changed() {
        return;
    }

    for entity in &existing {
        commands.entity(entity).despawn();
    }
    if layout.pressure_plates.is_empty() {
        return;
    }

    let plate_assets = plate_assets.get_or_insert_with(|| {
        PlateAssets::new(&mut meshes, &mut materials, &asset_server, &asset_set, &client_settings)
    });

    for plate in &layout.pressure_plates {
        let floor_y = f32::from(plate.level) * LEVEL_HEIGHT + PLATE_Y_OFFSET;
        commands
            .spawn((
                PressurePlateMarker,
                PlatePurposeMarker(plate.purpose),
                MapLevel(plate.level),
                Transform::from_translation(Vec3::new(plate.center_x, floor_y, plate.center_z)),
                plate_visibility(plate.purpose, &locked.0),
            ))
            .with_children(|parent| {
                parent.spawn((
                    Mesh3d(plate_assets.frame_mesh.clone()),
                    MeshMaterial3d(plate_assets.frame_material.clone()),
                    Transform::from_xyz(0.0, PLATE_FRAME_HEIGHT / 2.0, 0.0),
                ));
                parent.spawn((
                    Mesh3d(plate_assets.panel_mesh.clone()),
                    MeshMaterial3d(plate_assets.panel_material.clone()),
                    Transform::from_xyz(
                        0.0,
                        PLATE_FRAME_HEIGHT + PLATE_PANEL_HEIGHT / 2.0 - PLATE_PANEL_OVERLAP,
                        0.0,
                    ),
                ));
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plate_box_top_spans_its_tile_and_has_tangents() {
        let mesh = plate_box(1.2, 0.05, 1.2);
        let uvs = match mesh.attribute(Mesh::ATTRIBUTE_UV_0) {
            Some(bevy::mesh::VertexAttributeValues::Float32x2(uvs)) => uvs,
            other => panic!("unexpected UV attribute: {other:?}"),
        };
        let min_u = uvs.iter().map(|uv| uv[0]).fold(f32::MAX, f32::min);
        let max_u = uvs.iter().map(|uv| uv[0]).fold(f32::MIN, f32::max);
        assert!(
            (max_u - min_u - 1.0).abs() < 1e-5,
            "one tile across the panel: {min_u}..{max_u}"
        );
        assert!(mesh.attribute(Mesh::ATTRIBUTE_TANGENT).is_some());
    }
}
