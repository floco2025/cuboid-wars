use bevy::prelude::*;

use crate::{
    constants::*,
    markers::*,
    resources::{CameraViewMode, DebugColors, LevelFocusEnabled},
    spawning::{spawn_floor, spawn_ramp, spawn_wall, spawn_wall_light_from_layout},
};
use common::protocol::MapLayout;

// ============================================================================
// World Geometry Setup System
// ============================================================================

pub fn setup_world_geometry_system(mut commands: Commands) {
    commands.spawn((
        DirectionalLight {
            illuminance: LIGHT_DIRECTIONAL_BRIGHTNESS,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(5.0, 15.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    commands.insert_resource(GlobalAmbientLight {
        color: Color::WHITE,
        brightness: LIGHT_AMBIENT_BRIGHTNESS,
        affects_lightmapped_meshes: false,
    });
}

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
    debug_colors: Res<DebugColors>,
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
        "spawning {} walls, {} floors, {} ramps",
        map_layout.walls.len(),
        map_layout.floors.len(),
        map_layout.ramps.len(),
    );

    for wall in &map_layout.walls {
        spawn_wall(
            &mut commands,
            &mut meshes,
            &mut materials,
            &asset_server,
            wall,
            debug_colors.0,
        );
    }

    for light in &map_layout.wall_lights {
        spawn_wall_light_from_layout(&mut commands, &asset_server, light);
    }

    for floor in &map_layout.floors {
        spawn_floor(
            &mut commands,
            &mut meshes,
            &mut materials,
            &asset_server,
            floor,
            debug_colors.0,
        );
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
// Level Focus Visibility System
// ============================================================================

// When `LevelFocusEnabled` is on, hide walls/floors at any level other than
// the local player's, and hide ramps that don't connect to the local player's
// level. When off, show everything. Runs every frame because the player's
// level can change as they walk up/down ramps.
pub fn map_level_focus_visibility_system(
    focus: Res<LevelFocusEnabled>,
    local_player: Query<&common::protocol::Position, With<LocalPlayerMarker>>,
    mut walls_floors: Query<
        (&MapLevel, &mut Visibility),
        (
            Or<(With<WallMarker>, With<RoofMarker>, With<GroundMarker>)>,
            Without<RampMarker>,
        ),
    >,
    mut ramps: Query<(&MapLevel, &mut Visibility), With<RampMarker>>,
) {
    if !focus.0 {
        for (_, mut vis) in &mut walls_floors {
            *vis = Visibility::Visible;
        }
        for (_, mut vis) in &mut ramps {
            *vis = Visibility::Visible;
        }
        return;
    }

    let Ok(pos) = local_player.single() else {
        return;
    };
    let player_level = common::map::compute_player_level(pos.y);

    for (level, mut vis) in &mut walls_floors {
        *vis = if level.0 == player_level {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
    for (level, mut vis) in &mut ramps {
        // Show a ramp if it touches the player's level on either side.
        *vis = if level.0 == player_level || level.0 + 1 == player_level {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
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
            // Make it emissive using configurable fixture settings
            let warm_tint = (1.0, 0.95, 0.85);
            material.emissive = LinearRgba::rgb(
                warm_tint.0 * WALL_LIGHT_EMISSIVE_LUMINANCE,
                warm_tint.1 * WALL_LIGHT_EMISSIVE_LUMINANCE,
                warm_tint.2 * WALL_LIGHT_EMISSIVE_LUMINANCE,
            );
            material.base_color = Color::srgba(warm_tint.0, warm_tint.1, warm_tint.2, material.base_color.alpha());
            processed.insert(id);
        }
    }
}
