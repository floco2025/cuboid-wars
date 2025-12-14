use bevy::prelude::*;

use crate::{
    constants::{TOPDOWN_ROOF_ALPHA, TOPDOWN_WALL_ALPHA},
    resources::{CameraViewMode, RoofRenderingEnabled, WallConfig},
    spawning::{spawn_roof, spawn_wall},
};

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

// System to spawn walls and roofs when WallConfig is available
pub fn map_spawn_walls_system(
    mut commands: Commands,
    wall_config: Option<Res<WallConfig>>,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut spawned: Local<bool>,
) {
    // Spawn exactly once after the server shares its wall configuration
    let Some(wall_config) = wall_config else {
        return;
    };

    if *spawned {
        return;
    }

    info!(
        "spawning {} wall segments and {} roofs",
        wall_config.all_walls.len(),
        wall_config.roofs.len()
    );

    for wall in &wall_config.all_walls {
        spawn_wall(&mut commands, &mut meshes, &mut materials, &asset_server, wall);
    }

    for roof in &wall_config.roofs {
        spawn_roof(&mut commands, &mut meshes, &mut materials, &asset_server, roof);
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
