use bevy::prelude::*;

use crate::{
    constants::{TOPDOWN_ROOF_ALPHA, TOPDOWN_WALL_ALPHA},
    resources::{CameraViewMode, RoofRenderingEnabled},
    spawning::{spawn_ramp, spawn_roof, spawn_wall, spawn_wall_light_from_layout},
};
use common::protocol::MapLayout;

// ============================================================================
// Components
// ============================================================================

// Marker component for walls
#[derive(Component)]
pub struct WallMarker;

// Marker component for roofs
#[derive(Component)]
pub struct RoofMarker;

// ============================================================================
// Wall Spawning System
// ============================================================================

// System to spawn walls and roofs when GridConfig is available
pub fn map_spawn_walls_system(
    mut commands: Commands,
    map_layout: Option<Res<MapLayout>>,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut spawned: Local<bool>,
) {
    // Spawn exactly once after the server shares its wall configuration
    let Some(map_layout) = map_layout else {
        return;
    };

    if *spawned {
        return;
    }

    info!(
        "spawning {} wall segments, {} roofs, {} ramps",
        map_layout.lower_walls.len(),
        map_layout.roofs.len(),
        map_layout.ramps.len()
    );

    for wall in &map_layout.lower_walls {
        spawn_wall(&mut commands, &mut meshes, &mut materials, &asset_server, wall);
    }

    for light in &map_layout.wall_lights {
        spawn_wall_light_from_layout(&mut commands, &asset_server, light);
    }

    for roof in &map_layout.roofs {
        spawn_roof(&mut commands, &mut meshes, &mut materials, &asset_server, roof);
    }

    for ramp in &map_layout.ramps {
        spawn_ramp(&mut commands, &mut meshes, &mut materials, &asset_server, ramp);
    }

    *spawned = true;
}

// ============================================================================
// Wall Opacity System
// ============================================================================

// System to toggle wall and roof opacity based on camera view mode
pub fn map_toggle_wall_opacity_system(
    view_mode: Res<CameraViewMode>,
    wall_query: Query<&MeshMaterial3d<StandardMaterial>, With<WallMarker>>,
    roof_query: Query<&MeshMaterial3d<StandardMaterial>, With<RoofMarker>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if !view_mode.is_changed() {
        return;
    }

    match *view_mode {
        CameraViewMode::FirstPerson => {
            // Walls and roofs fully opaque in first-person
            for material_handle in wall_query.iter().chain(roof_query.iter()) {
                if let Some(material) = materials.get_mut(&material_handle.0) {
                    material.base_color.set_alpha(1.0);
                    material.alpha_mode = AlphaMode::Opaque;
                }
            }
        }
        CameraViewMode::TopDown => {
            // Walls - use Blend for transparency, Opaque for alpha=1.0
            for material_handle in &wall_query {
                if let Some(material) = materials.get_mut(&material_handle.0) {
                    material.base_color.set_alpha(TOPDOWN_WALL_ALPHA);
                    material.alpha_mode = if TOPDOWN_WALL_ALPHA >= 1.0 {
                        AlphaMode::Opaque
                    } else {
                        AlphaMode::Blend
                    };
                }
            }
            // Roofs - use Blend for transparency, Opaque for alpha=1.0 to prevent Z-fighting
            for material_handle in &roof_query {
                if let Some(material) = materials.get_mut(&material_handle.0) {
                    material.base_color.set_alpha(TOPDOWN_ROOF_ALPHA);
                    material.alpha_mode = if TOPDOWN_ROOF_ALPHA >= 1.0 {
                        AlphaMode::Opaque
                    } else {
                        AlphaMode::Blend
                    };
                }
            }
        }
    }
}

// ============================================================================
// Roof Opacity System
// ============================================================================

// System to toggle roof visibility based on RoofRenderingEnabled resource
pub fn map_toggle_roof_visibility_system(
    roof_enabled: Res<RoofRenderingEnabled>,
    mut roof_query: Query<&mut Visibility, With<RoofMarker>>,
) {
    if !roof_enabled.is_changed() {
        return;
    }

    let visibility = if roof_enabled.0 {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };

    for mut vis in &mut roof_query {
        *vis = visibility;
    }
}

// ============================================================================
// Wall Light Emissive System
// ============================================================================

// System to make wall light glass materials emissive after they load
pub fn map_make_wall_lights_emissive_system(
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut processed: Local<std::collections::HashSet<AssetId<StandardMaterial>>>,
) {
    // Check all materials for ones that look like wall light glass
    for (id, material) in materials.iter_mut() {
        // Skip if already processed
        if processed.contains(&id) {
            continue;
        }

        // Check if this material has properties suggesting it's glass
        // (typically has some transparency or specific naming)
        if material.alpha_mode != AlphaMode::Opaque || material.base_color.alpha() < 1.0 {
            // Make it emissive
            material.emissive = LinearRgba::rgb(10.0, 9.5, 8.0); // Bright warm white
            material.base_color = Color::srgba(1.0, 0.95, 0.85, material.base_color.alpha());
            processed.insert(id);
        }
    }
}
