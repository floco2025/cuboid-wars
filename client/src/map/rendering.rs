use bevy::light::{DirectionalLightShadowMap, cluster::GlobalClusterSettings};
use bevy::prelude::*;
use std::collections::HashSet;

use crate::{
    barriers::{BarrierAssets, BarrierMarker},
    config::{AssetSet, ClientSettings},
    map::{
        DebugColorMode, DebugColors, GrassMarker, GroundMarker, LadderMarker, LevelFocusEnabled, MapGeometryBatch,
        MapLevel, RampMarker, RoofMarker, WallLightMarker, WallMarker, batch_floor, batch_ramp, batch_wall,
        spawn_ladder_from_layout, spawn_wall_light_from_layout,
    },
    materials::MaterialHandleCache,
    players::LocalPlayerMarker,
};
use common::protocol::{ItemMarker, MapLayout};

// ============================================================================
// Scene Lighting Setup System
// ============================================================================

// GPU clustering's Z-slice list defaults to 1024, which our dense lit scenes
// overflow; Bevy then resizes mid-render and corrupts lighting for a few frames.
// Pre-size it (PbrPlugin builds the resource in `finish`, so we mutate, not insert).
const CLUSTER_Z_SLICE_CAPACITY: usize = 8192;

pub fn setup_scene_lighting_system(
    mut commands: Commands,
    client_settings: Res<ClientSettings>,
    mut cluster_settings: ResMut<GlobalClusterSettings>,
) {
    commands.spawn((
        DirectionalLight {
            illuminance: client_settings.lighting.bright.sun_illuminance,
            shadow_maps_enabled: client_settings.rendering.directional_shadows,
            ..default()
        },
        Transform::from_xyz(5.0, 15.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
        super::skybox::SunLightMarker,
    ));

    commands.insert_resource(GlobalAmbientLight {
        color: Color::WHITE,
        brightness: client_settings.lighting.bright.ambient_brightness,
        affects_lightmapped_meshes: false,
    });
    commands.insert_resource(DirectionalLightShadowMap {
        size: client_settings.rendering.shadow_map_size as usize,
    });

    if let Some(gpu) = cluster_settings.gpu_clustering.as_mut() {
        gpu.initial_z_slice_list_capacity = gpu.initial_z_slice_list_capacity.max(CLUSTER_Z_SLICE_CAPACITY);
    }
}

// ============================================================================
// Map Geometry Spawning System
// ============================================================================

// System to spawn static map geometry once the server shares its map layout.
// Re-runs when `DebugColors` changes so the user can cycle modes at runtime.
pub fn map_spawn_geometry_system(
    mut commands: Commands,
    map_layout: Res<MapLayout>,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_set: Res<AssetSet>,
    client_settings: Res<ClientSettings>,
    debug_colors: Res<DebugColors>,
    map_entities: Query<
        Entity,
        Or<(
            With<WallMarker>,
            With<GroundMarker>,
            With<RoofMarker>,
            With<RampMarker>,
            With<WallLightMarker>,
            With<LadderMarker>,
        )>,
    >,
    mut last_spawn: Local<Option<DebugColorMode>>,
    mut material_cache: Local<MaterialHandleCache>,
) {
    if last_spawn.as_ref() == Some(&debug_colors.0) {
        return;
    }

    // Despawn any geometry from a previous spawn so a re-spawn doesn't double up.
    for entity in &map_entities {
        commands.entity(entity).despawn();
    }
    *material_cache = MaterialHandleCache::default();

    info!("spawning {}", map_layout.summary());

    let mut geometry = MapGeometryBatch::new(debug_colors.0);

    for (wall, materials) in map_layout.walls.iter().zip(map_layout.wall_materials.iter()) {
        batch_wall(&mut geometry, &asset_set, wall, materials);
    }

    for light in &map_layout.wall_lights {
        spawn_wall_light_from_layout(&mut commands, &asset_server, &asset_set, light);
    }

    if !map_layout.ladders.is_empty() {
        let ladder_material_id = asset_set.ladder_material_id();
        let ladder_material = material_cache.standard(
            ladder_material_id,
            asset_set.ladder_material_def(),
            &asset_server,
            &mut materials,
            client_settings.rendering.texture_anisotropy,
            client_settings.rendering.mipmaps,
        );
        let ladder_tile_size = asset_set.ladder_material_def().tile_size();
        for ladder in &map_layout.ladders {
            spawn_ladder_from_layout(
                &mut commands,
                &mut meshes,
                ladder_material.clone(),
                ladder_tile_size,
                ladder,
            );
        }
    }

    for (floor, materials) in map_layout.floors.iter().zip(map_layout.floor_materials.iter()) {
        batch_floor(&mut geometry, &asset_set, floor, materials);
    }

    for (ramp, materials) in map_layout.ramps.iter().zip(map_layout.ramp_materials.iter()) {
        batch_ramp(&mut geometry, &asset_set, ramp, materials);
    }

    info!(
        "batched map into {} mesh entities, {} triangles",
        geometry.batch_count(),
        geometry.triangle_count(),
    );

    geometry.flush(
        &mut commands,
        &mut meshes,
        &mut materials,
        &mut material_cache,
        &asset_server,
        &asset_set,
        &client_settings,
    );

    *last_spawn = Some(debug_colors.0);
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
                With<BarrierMarker>,
                With<GrassMarker>,
            )>,
            Without<RampMarker>,
            Without<LadderMarker>,
        ),
    >,
    mut ramps: Query<(&MapLevel, &mut Visibility), (With<RampMarker>, Without<LadderMarker>)>,
    mut ladders: Query<(&MapLevel, &LadderMarker, &mut Visibility)>,
) {
    // `set_if_neq` everywhere: writing every `Visibility` each frame would
    // mark hundreds of unchanged entities dirty and re-run visibility
    // propagation for all of them.
    if !focus.0 {
        for (_, mut vis) in &mut level_entities {
            vis.set_if_neq(Visibility::Visible);
        }
        for (_, mut vis) in &mut ramps {
            vis.set_if_neq(Visibility::Visible);
        }
        for (_, _, mut vis) in &mut ladders {
            vis.set_if_neq(Visibility::Visible);
        }
        return;
    }

    let Ok(pos) = local_player.single() else {
        return;
    };
    let player_level = visual_focus_level(pos.y);

    for (level, mut vis) in &mut level_entities {
        vis.set_if_neq(if level.0 == player_level {
            Visibility::Visible
        } else {
            Visibility::Hidden
        });
    }
    for (level, mut vis) in &mut ramps {
        // Show a ramp if it touches the player's level on either side.
        vis.set_if_neq(if level.0 == player_level || level.0 + 1 == player_level {
            Visibility::Visible
        } else {
            Visibility::Hidden
        });
    }
    for (level, ladder, mut vis) in &mut ladders {
        // Show a ladder from every level it spans, top landing included.
        vis.set_if_neq(if (level.0..=level.0 + ladder.levels).contains(&player_level) {
            Visibility::Visible
        } else {
            Visibility::Hidden
        });
    }
}

// ============================================================================
// Wall Light Emissive System
// ============================================================================

// System to make wall light glass materials emissive after they load
pub fn map_wall_light_emissive_system(
    asset_set: Res<AssetSet>,
    barrier_assets: Res<BarrierAssets>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut processed: Local<HashSet<AssetId<StandardMaterial>>>,
) {
    // Barrier materials match the "non-opaque" heuristic below but are
    // managed by their own pulsation system — keep this pass off them.
    for handle in barrier_assets.material_handles() {
        processed.insert(handle.id());
    }

    let emissive_luminance = asset_set.wall_light_model().emissive_luminance;

    // Find unprocessed glass materials read-only first. `iter_mut()` flags
    // EVERY returned material as `Modified` regardless of edits, forcing a
    // per-frame GPU re-extract/re-prepare of all materials; `iter()` queues no
    // change events. Once every glass material is processed this yields nothing
    // and no material is touched again.
    let candidates: Vec<AssetId<StandardMaterial>> = materials
        .iter()
        .filter(|(id, material)| {
            !processed.contains(id) && (material.alpha_mode != AlphaMode::Opaque || material.base_color.alpha() < 1.0)
        })
        .map(|(id, _)| id)
        .collect();

    // Make wall-light glass emissive using configurable fixture settings.
    let warm_tint = (1.0, 0.95, 0.85);
    for id in candidates {
        let Some(mut material) = materials.get_mut(id) else {
            continue;
        };
        material.emissive = LinearRgba::rgb(
            warm_tint.0 * emissive_luminance,
            warm_tint.1 * emissive_luminance,
            warm_tint.2 * emissive_luminance,
        );
        material.base_color = Color::srgba(warm_tint.0, warm_tint.1, warm_tint.2, material.base_color.alpha());
        processed.insert(id);
    }
}

pub(crate) fn visual_focus_level(y: f32) -> u8 {
    if y <= 0.0 {
        return 0;
    }
    (y / common::constants::LEVEL_HEIGHT).round().min(f32::from(u8::MAX)) as u8
}
