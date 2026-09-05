use bevy::asset::AssetPath;
use bevy::light::{DirectionalLightShadowMap, cluster::GlobalClusterSettings};
use bevy::prelude::*;

use crate::{
    bridges::LightBridgeMarker,
    config::{AssetSet, ClientSettings},
    map::{
        DebugColorMode, DebugColors, FocusedMapLevel, GrassMarker, GroundMarker, LadderMarker, LevelFocusEnabled,
        MapGeometryBatch, MapLevel, RampMarker, RoofMarker, WallLightMarker, WallMarker, batch_floor, batch_ramp,
        batch_wall, spawn_ladder_from_layout, spawn_wall_light_from_layout,
    },
    materials::MaterialHandleCache,
    moving_floors::MovingFloorMarker,
    players::LocalPlayerMarker,
};
use common::protocol::{ItemMarker, MapLayout, MapSettings};

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
    map_settings: Res<MapSettings>,
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
        spawn_wall_light_from_layout(&mut commands, &asset_server, &asset_set, map_settings.geometry, light);
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
        batch_ramp(&mut geometry, &asset_set, map_settings.geometry, ramp, materials);
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

pub fn update_focused_map_level_system(
    focus: Res<LevelFocusEnabled>,
    map_settings: Res<MapSettings>,
    local_player: Query<&common::protocol::Position, With<LocalPlayerMarker>>,
    mut focused: ResMut<FocusedMapLevel>,
) {
    // Reduce continuous player movement to the level value that controls the large visibility pass.
    let focused_level = if focus.0 {
        local_player
            .single()
            .ok()
            .map(|position| map_settings.geometry.nearest_level_to_y(position.y))
    } else {
        None
    };
    // Equal writes would rerun visibility updates across the entire map.
    focused.set_if_neq(FocusedMapLevel(focused_level));
}

pub fn map_level_focus_visibility_system(
    focused: Res<FocusedMapLevel>,
    mut level_entities: Query<
        (&MapLevel, &mut Visibility),
        (
            Or<(
                With<WallMarker>,
                With<RoofMarker>,
                With<GroundMarker>,
                With<WallLightMarker>,
                With<ItemMarker>,
                With<GrassMarker>,
                With<LightBridgeMarker>,
            )>,
            Without<RampMarker>,
            Without<LadderMarker>,
            Without<MovingFloorMarker>,
        ),
    >,
    mut ramps: Query<
        (&MapLevel, &mut Visibility),
        (With<RampMarker>, Without<LadderMarker>, Without<MovingFloorMarker>),
    >,
    mut ladders: Query<(&MapLevel, &LadderMarker, &mut Visibility), Without<MovingFloorMarker>>,
    mut moving_floors: Query<(&MapLevel, &MovingFloorMarker, &mut Visibility)>,
) {
    // Equal writes would retrigger visibility propagation for hundreds of map entities.
    for (level, mut vis) in &mut level_entities {
        vis.set_if_neq(level_visibility(*focused, level.0));
    }
    for (level, mut vis) in &mut ramps {
        vis.set_if_neq(ramp_visibility(*focused, level.0));
    }
    for (level, ladder, mut vis) in &mut ladders {
        vis.set_if_neq(ladder_visibility(*focused, level.0, ladder.levels));
    }
    for (level, tile, mut vis) in &mut moving_floors {
        vis.set_if_neq(ladder_visibility(*focused, level.0, tile.levels));
    }
}

// Map entities can spawn without a level transition, so they still need the current visibility.
pub fn added_map_level_visibility_system(
    focused: Res<FocusedMapLevel>,
    mut level_entities: Query<
        (&MapLevel, &mut Visibility),
        (
            Added<MapLevel>,
            Or<(
                With<WallMarker>,
                With<RoofMarker>,
                With<GroundMarker>,
                With<WallLightMarker>,
                With<ItemMarker>,
                With<GrassMarker>,
                With<LightBridgeMarker>,
            )>,
            Without<RampMarker>,
            Without<LadderMarker>,
            Without<MovingFloorMarker>,
        ),
    >,
    mut ramps: Query<
        (&MapLevel, &mut Visibility),
        (
            Added<MapLevel>,
            With<RampMarker>,
            Without<LadderMarker>,
            Without<MovingFloorMarker>,
        ),
    >,
    mut ladders: Query<
        (&MapLevel, &LadderMarker, &mut Visibility),
        (Added<MapLevel>, With<LadderMarker>, Without<MovingFloorMarker>),
    >,
    mut moving_floors: Query<(&MapLevel, &MovingFloorMarker, &mut Visibility), Added<MapLevel>>,
) {
    for (level, mut visibility) in &mut level_entities {
        *visibility = level_visibility(*focused, level.0);
    }
    for (level, mut visibility) in &mut ramps {
        *visibility = ramp_visibility(*focused, level.0);
    }
    for (level, ladder, mut visibility) in &mut ladders {
        *visibility = ladder_visibility(*focused, level.0, ladder.levels);
    }
    for (level, tile, mut visibility) in &mut moving_floors {
        *visibility = ladder_visibility(*focused, level.0, tile.levels);
    }
}

fn level_visibility(focused: FocusedMapLevel, level: u8) -> Visibility {
    match focused.0 {
        Some(focused_level) if level != focused_level => Visibility::Hidden,
        _ => Visibility::Visible,
    }
}

fn ramp_visibility(focused: FocusedMapLevel, level: u8) -> Visibility {
    match focused.0 {
        Some(focused_level) if level != focused_level && level.saturating_add(1) != focused_level => Visibility::Hidden,
        _ => Visibility::Visible,
    }
}

fn ladder_visibility(focused: FocusedMapLevel, level: u8, levels: u8) -> Visibility {
    match focused.0 {
        Some(focused_level) if !(level..=level.saturating_add(levels)).contains(&focused_level) => Visibility::Hidden,
        _ => Visibility::Visible,
    }
}

// ============================================================================
// Wall Light Emissive System
// ============================================================================

// System to make wall light glass materials emissive after they load
pub fn map_wall_light_emissive_system(
    asset_set: Res<AssetSet>,
    asset_server: Res<AssetServer>,
    mut asset_events: MessageReader<AssetEvent<StandardMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let wall_light = asset_set.wall_light_model();
    let scene_path = AssetPath::parse(&wall_light.scene);
    let emissive_luminance = wall_light.emissive_luminance;
    let warm_tint = (1.0, 0.95, 0.85);
    let desired_emissive = LinearRgba::rgb(
        warm_tint.0 * emissive_luminance,
        warm_tint.1 * emissive_luminance,
        warm_tint.2 * emissive_luminance,
    );
    // Material events avoid rescanning every loaded material while still catching late asset loads.
    for event in asset_events.read() {
        let Some(id) = (match event {
            AssetEvent::Added { id } | AssetEvent::Modified { id } | AssetEvent::LoadedWithDependencies { id } => {
                Some(*id)
            }
            AssetEvent::Removed { .. } | AssetEvent::Unused { .. } => None,
        }) else {
            continue;
        };
        // Only the lamp glb's own sub-materials; every other translucent material has its own owner.
        if asset_server
            .get_path(id)
            .is_none_or(|path| path.path() != scene_path.path())
        {
            continue;
        }
        let Some(material) = materials.get(id) else {
            continue;
        };
        if material.alpha_mode == AlphaMode::Opaque {
            continue;
        }
        let desired_base_color = Color::srgba(warm_tint.0, warm_tint.1, warm_tint.2, material.base_color.alpha());
        // Avoid emitting another Modified event in response to this system's own write.
        if material.emissive == desired_emissive && material.base_color == desired_base_color {
            continue;
        }
        let Some(mut material) = materials.get_mut(id) else {
            continue;
        };
        material.emissive = desired_emissive;
        material.base_color = desired_base_color;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focused_level_visibility_includes_connecting_ramps_and_ladders() {
        let focused = FocusedMapLevel(Some(2));

        assert_eq!(level_visibility(FocusedMapLevel(None), 1), Visibility::Visible);
        assert_eq!(level_visibility(focused, 2), Visibility::Visible);
        assert_eq!(level_visibility(focused, 1), Visibility::Hidden);
        assert_eq!(ramp_visibility(focused, 1), Visibility::Visible);
        assert_eq!(ramp_visibility(focused, 0), Visibility::Hidden);
        assert_eq!(ladder_visibility(focused, 0, 2), Visibility::Visible);
        assert_eq!(ladder_visibility(focused, 0, 1), Visibility::Hidden);
    }

    #[test]
    fn a_light_bridge_follows_level_focus_like_a_floor() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(FocusedMapLevel(Some(1)))
            .add_systems(
                Update,
                (map_level_focus_visibility_system, added_map_level_visibility_system).chain(),
            );
        let bridge = app
            .world_mut()
            .spawn((LightBridgeMarker, MapLevel(2), Visibility::Visible))
            .id();
        let visibility = |app: &App| {
            *app.world()
                .get::<Visibility>(bridge)
                .expect("bridge lost its visibility")
        };

        app.update();
        assert_eq!(visibility(&app), Visibility::Hidden);

        app.insert_resource(FocusedMapLevel(Some(2)));
        app.update();
        assert_eq!(visibility(&app), Visibility::Visible);
    }

    #[test]
    fn a_moving_floor_follows_level_focus_like_a_ladder() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(FocusedMapLevel(Some(1)))
            .add_systems(
                Update,
                (map_level_focus_visibility_system, added_map_level_visibility_system).chain(),
            );
        let lift = app
            .world_mut()
            .spawn((
                MovingFloorMarker { index: 0, levels: 1 },
                MapLevel(2),
                Visibility::Visible,
            ))
            .id();
        let visibility = |app: &App| *app.world().get::<Visibility>(lift).expect("lift lost its visibility");

        app.update();
        assert_eq!(visibility(&app), Visibility::Hidden);

        for focused in [2, 3] {
            app.insert_resource(FocusedMapLevel(Some(focused)));
            app.update();
            assert_eq!(visibility(&app), Visibility::Visible, "focused on {focused}");
        }
    }
}
