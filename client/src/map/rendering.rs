use bevy::{
    asset::AssetPath,
    light::{DirectionalLightShadowMap, cluster::GlobalClusterSettings},
    prelude::*,
};

use super::skybox::{CelestialLightMarker, selected_skybox};
use crate::fields::{CheckpointMarker, EraserMarker};
use crate::{
    bridges::LightBridgeMarker,
    carriers::{CarrierEntities, CarrierStoreys},
    config::{AssetSet, ClientSettings},
    map::{
        DebugColorMode, DebugColors, FocusedMapLevel, GrassMarker, GroundMarker, LadderMarker, LevelFocusEnabled,
        MapGeometryBatch, MapLevel, RampMarker, RoofMarker, WallLightMarker, WallMarker, batch_floor, batch_ramp,
        batch_wall, spawn_ladder_from_layout, spawn_wall_light_from_layout,
    },
    materials::MaterialHandleCache,
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
    asset_set: Res<AssetSet>,
    map_settings: Res<MapSettings>,
    mut cluster_settings: ResMut<GlobalClusterSettings>,
) {
    let celestial = selected_skybox(&asset_set, &map_settings).celestial_disc;
    commands.spawn((
        DirectionalLight {
            illuminance: client_settings.lighting.bright.sun_illuminance,
            shadow_maps_enabled: client_settings.rendering.directional_shadows,
            ..default()
        },
        Transform::default().looking_to(-Vec3::from_array(celestial.direction), Vec3::Y),
        CelestialLightMarker,
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
    carrier_entities: Res<CarrierEntities>,
    storeys: Res<CarrierStoreys>,
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
        batch_wall(&mut geometry, &asset_set, &storeys, wall, materials);
    }

    for light in &map_layout.wall_lights {
        spawn_wall_light_from_layout(
            &mut commands,
            &asset_server,
            &asset_set,
            map_settings.geometry,
            &storeys,
            carrier_entities.get(light.carrier),
            light,
        );
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
                &storeys,
                carrier_entities.get(ladder.carrier),
                ladder,
            );
        }
    }

    for (floor, materials) in map_layout.floors.iter().zip(map_layout.floor_materials.iter()) {
        batch_floor(&mut geometry, &asset_set, &storeys, floor, materials);
    }

    for (ramp, materials) in map_layout.ramps.iter().zip(map_layout.ramp_materials.iter()) {
        batch_ramp(
            &mut geometry,
            &asset_set,
            map_settings.geometry,
            &storeys,
            ramp,
            materials,
        );
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
        &carrier_entities,
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

// Every map entity that follows level focus; barriers and pressure plates
// have their own owners, which combine the focus with plate state.
type MapLevelFilter = Or<(
    With<WallMarker>,
    With<RoofMarker>,
    With<GroundMarker>,
    With<WallLightMarker>,
    With<ItemMarker>,
    With<GrassMarker>,
    With<LightBridgeMarker>,
    With<EraserMarker>,
    With<CheckpointMarker>,
    With<RampMarker>,
    With<LadderMarker>,
)>;

pub fn map_level_focus_visibility_system(
    focused: Res<FocusedMapLevel>,
    mut level_entities: Query<(&MapLevel, &mut Visibility), MapLevelFilter>,
) {
    // Equal writes would retrigger visibility propagation for hundreds of map entities.
    for (level, mut vis) in &mut level_entities {
        vis.set_if_neq(map_level_visibility(*focused, *level));
    }
}

// Map entities can spawn without a level transition, so they still need the current visibility.
pub fn added_map_level_visibility_system(
    focused: Res<FocusedMapLevel>,
    mut level_entities: Query<(&MapLevel, &mut Visibility), (Added<MapLevel>, MapLevelFilter)>,
) {
    for (level, mut visibility) in &mut level_entities {
        *visibility = map_level_visibility(*focused, *level);
    }
}

// The one level-focus rule: an entity shows while the focused storey is one
// of those it belongs to.
#[must_use]
pub(crate) fn map_level_visibility(focused: FocusedMapLevel, level: MapLevel) -> Visibility {
    match focused.0 {
        Some(focused_level) if !level.contains(focused_level) => Visibility::Hidden,
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
        let Some(path) = asset_server.get_path(id) else {
            continue;
        };
        let Some(light) = asset_set
            .wall_light_models()
            .find(|light| AssetPath::parse(&light.scene).path() == path.path())
        else {
            continue;
        };
        let [r, g, b] = light.color;
        let desired_emissive = LinearRgba::rgb(r, g, b) * light.emissive_luminance;
        let Some(material) = materials.get(id) else {
            continue;
        };
        if material.emissive == LinearRgba::BLACK {
            continue;
        }
        let desired_base_color = Color::srgba(r, g, b, material.base_color.alpha());
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
#[path = "tests/rendering.rs"]
mod tests;
