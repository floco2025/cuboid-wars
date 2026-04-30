use bevy::prelude::*;

use crate::{
    config::AssetSet,
    constants::*,
    markers::*,
    resources::{DebugColors, LevelFocusEnabled},
    spawning::{MapMaterialCache, MapMeshBatcher, spawn_floor, spawn_ramp, spawn_wall, spawn_wall_light_from_layout},
    systems::visual_focus_level,
};
use common::{markers::ItemMarker, protocol::MapLayout};

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
    asset_set: Res<AssetSet>,
    debug_colors: Res<DebugColors>,
    mut spawned: Local<bool>,
    mut material_cache: Local<MapMaterialCache>,
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

    let mut batcher = MapMeshBatcher::default();

    for wall in &map_layout.walls {
        spawn_wall(&mut batcher, &asset_set, wall);
    }

    for light in &map_layout.wall_lights {
        spawn_wall_light_from_layout(&mut commands, &asset_server, &asset_set, light);
    }

    for floor in &map_layout.floors {
        spawn_floor(&mut batcher, &asset_set, floor);
    }

    for ramp in &map_layout.ramps {
        spawn_ramp(&mut batcher, &asset_set, ramp);
    }

    info!(
        "batched map into {} mesh entities, {} triangles",
        batcher.batch_count(),
        batcher.triangle_count(),
    );

    batcher.flush(
        &mut commands,
        &mut meshes,
        &mut materials,
        &mut material_cache,
        &asset_server,
        &asset_set,
        debug_colors.0,
    );

    *spawned = true;
}

// ============================================================================
// Level Focus Visibility System
// ============================================================================

// When `LevelFocusEnabled` is on, hide level-bound map entities at any level
// other than the local player's, and hide ramps that don't connect to the local
// player's level. When off, show everything. Runs every frame because the
// player's level can change as they walk up/down ramps.
pub fn map_level_focus_visibility_system(
    focus: Res<LevelFocusEnabled>,
    local_player: Query<&common::protocol::Position, With<LocalPlayerMarker>>,
    mut level_entities: Query<
        (&MapLevel, &mut Visibility),
        (
            Or<(
                With<WallMarker>,
                With<RoofMarker>,
                With<GroundMarker>,
                With<WallLightMarker>,
                With<ItemMarker>,
            )>,
            Without<RampMarker>,
        ),
    >,
    mut ramps: Query<(&MapLevel, &mut Visibility), With<RampMarker>>,
) {
    if !focus.0 {
        for (_, mut vis) in &mut level_entities {
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
    let player_level = visual_focus_level(pos.y);

    for (level, mut vis) in &mut level_entities {
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
    asset_set: Res<AssetSet>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut processed: Local<std::collections::HashSet<AssetId<StandardMaterial>>>,
) {
    let emissive_luminance = asset_set.wall_light_model().emissive_luminance;

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
                warm_tint.0 * emissive_luminance,
                warm_tint.1 * emissive_luminance,
                warm_tint.2 * emissive_luminance,
            );
            material.base_color = Color::srgba(warm_tint.0, warm_tint.1, warm_tint.2, material.base_color.alpha());
            processed.insert(id);
        }
    }
}
