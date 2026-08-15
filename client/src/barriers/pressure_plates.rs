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
const PLATE_Y_OFFSET: f32 = 0.01;
// Housing: a dark base slab spanning the footprint, with the kind-colored
// button inset on top — a physical mechanism rather than a painted decal.
const PLATE_BASE_HEIGHT: f32 = 0.05;
const PLATE_BUTTON_SIDE: f32 = PLATE_SIDE * 0.7;
const PLATE_BUTTON_HEIGHT: f32 = 0.05;
// The button sinks this far into the base so no gap can show at the seam.
const PLATE_BUTTON_OVERLAP: f32 = 0.01;

// Shared across every plate: base/button meshes plus the neutral housing
// material (buttons use the per-kind plate materials from `BarrierAssets`).
// Pub only because it appears in the spawn system's `Local` parameter.
pub struct PlateAssets {
    base_mesh: Handle<Mesh>,
    button_mesh: Handle<Mesh>,
    base_material: Handle<StandardMaterial>,
}

// Spawn one housing per pressure plate in the current `MapLayout`, its
// button colored by barrier kind. Re-runs when the layout is inserted or
// replaced (e.g. reconnect / map change). Plates are static — no animation;
// the puzzle feedback is the barrier disappearing when the threshold is met.
pub fn pressure_plates_spawn_system(
    mut commands: Commands,
    map_layout: Option<Res<MapLayout>>,
    barrier_assets: Option<Res<BarrierAssets>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut plate_assets: Local<Option<PlateAssets>>,
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

    let plate_assets = plate_assets.get_or_insert_with(|| PlateAssets {
        base_mesh: meshes.add(Cuboid::new(PLATE_SIDE, PLATE_BASE_HEIGHT, PLATE_SIDE)),
        button_mesh: meshes.add(Cuboid::new(PLATE_BUTTON_SIDE, PLATE_BUTTON_HEIGHT, PLATE_BUTTON_SIDE)),
        base_material: materials.add(StandardMaterial {
            base_color: Color::srgb(0.12, 0.12, 0.13),
            perceptual_roughness: 0.6,
            metallic: 0.4,
            ..default()
        }),
    });

    for plate in &layout.pressure_plates {
        let floor_y = f32::from(plate.level) * LEVEL_HEIGHT + PLATE_Y_OFFSET;
        commands
            .spawn((
                PressurePlateMarker,
                MapLevel(plate.level),
                Transform::from_translation(Vec3::new(plate.center_x, floor_y, plate.center_z)),
                Visibility::Visible,
            ))
            .with_children(|parent| {
                parent.spawn((
                    Mesh3d(plate_assets.base_mesh.clone()),
                    MeshMaterial3d(plate_assets.base_material.clone()),
                    Transform::from_xyz(0.0, PLATE_BASE_HEIGHT / 2.0, 0.0),
                ));
                parent.spawn((
                    Mesh3d(plate_assets.button_mesh.clone()),
                    MeshMaterial3d(assets.material_for_plate(plate.kind).clone()),
                    Transform::from_xyz(
                        0.0,
                        PLATE_BASE_HEIGHT + PLATE_BUTTON_HEIGHT / 2.0 - PLATE_BUTTON_OVERLAP,
                        0.0,
                    ),
                ));
            });
    }
}
