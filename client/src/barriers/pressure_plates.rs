use bevy::prelude::*;

use super::BarrierAssets;
use crate::map::MapLevel;
use common::{
    constants::{GRID_CELL_SIZE, LEVEL_HEIGHT},
    protocol::MapLayout,
};

#[derive(Component)]
pub struct PressurePlateMarker;

// Plate footprint: inner 50% per side (≈25% by area). Slightly above the
// floor to avoid z-fighting with the floor slab beneath.
const PLATE_SIDE: f32 = GRID_CELL_SIZE * 0.5;
const PLATE_THICKNESS: f32 = 0.02;
const PLATE_Y_OFFSET: f32 = 0.01;

// Spawn one entity per pressure plate in the current `MapLayout`, colored by
// its barrier kind. Re-runs when the layout is inserted or replaced (e.g.
// reconnect / map change). Plates are static — no animation; the puzzle
// feedback is the barrier disappearing when the threshold is met.
pub fn pressure_plates_spawn_system(
    mut commands: Commands,
    map_layout: Option<Res<MapLayout>>,
    barrier_assets: Option<Res<BarrierAssets>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut plate_mesh: Local<Option<Handle<Mesh>>>,
    existing: Query<Entity, With<PressurePlateMarker>>,
) {
    let Some(layout) = map_layout else { return };
    let Some(assets) = barrier_assets else { return };
    if !layout.is_changed() {
        return;
    }

    for entity in &existing {
        commands.entity(entity).despawn();
    }
    if layout.pressure_plates.is_empty() {
        return;
    }

    let mesh = plate_mesh
        .get_or_insert_with(|| meshes.add(Cuboid::new(PLATE_SIDE, PLATE_THICKNESS, PLATE_SIDE)))
        .clone();

    for plate in &layout.pressure_plates {
        let center_y = f32::from(plate.level) * LEVEL_HEIGHT + PLATE_Y_OFFSET + PLATE_THICKNESS / 2.0;
        commands.spawn((
            PressurePlateMarker,
            MapLevel(plate.level),
            Mesh3d(mesh.clone()),
            MeshMaterial3d(assets.material_for(plate.kind).clone()),
            Transform::from_translation(Vec3::new(plate.center_x, center_y, plate.center_z)),
            Visibility::Visible,
        ));
    }
}
