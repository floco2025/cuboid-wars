use bevy::{math::Affine2, prelude::*};

use crate::{
    constants::*,
    markers::*,
    resources::{CameraViewMode, DebugColors, RoofRenderingEnabled},
    spawning::{
        load_repeating_texture, load_repeating_texture_linear, spawn_ramp, spawn_roof, spawn_roof_wall, spawn_wall,
        spawn_wall_light_from_layout,
    },
};
use common::{
    constants::{FIELD_DEPTH, FIELD_WIDTH},
    protocol::MapLayout,
};

// ============================================================================
// World Geometry Setup System
// ============================================================================

pub fn setup_world_geometry_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
) {
    // Create the ground plane
    let mut ground_mesh = Mesh::from(Plane3d::default().mesh().size(FIELD_WIDTH, FIELD_DEPTH));
    let _ = ground_mesh.generate_tangents();

    let uv_scale = Vec2::new(
        FIELD_WIDTH / TEXTURE_FLOOR_TILE_SIZE,
        FIELD_DEPTH / TEXTURE_FLOOR_TILE_SIZE,
    );

    commands.spawn((
        Mesh3d(meshes.add(ground_mesh)),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color_texture: Some(load_repeating_texture(&asset_server, "textures/ground/albedo.png")),
            normal_map_texture: Some(load_repeating_texture_linear(
                &asset_server,
                "textures/ground/normal-dx.png",
            )),
            occlusion_texture: Some(load_repeating_texture_linear(&asset_server, "textures/ground/ao.png")),
            metallic_roughness_texture: Some(load_repeating_texture_linear(
                &asset_server,
                "textures/ground/metallic-roughness.png",
            )),
            uv_transform: Affine2::from_scale(uv_scale),
            perceptual_roughness: TEXTURE_FLOOR_ROUGHNESS,
            metallic: TEXTURE_FLOOR_METALLIC,
            ..default()
        })),
        Transform::from_xyz(0.0, 0.0, 0.0),
        Visibility::default(),
    ));

    // Add soft directional light from above for shadows and definition
    commands.spawn((
        DirectionalLight {
            illuminance: LIGHT_DIRECTIONAL_BRIGHTNESS,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(5.0, 15.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // Add ambient light for diffuse fill lighting
    commands.insert_resource(AmbientLight {
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
        "spawning {} wall segments, {} roofs, {} ramps, {} roof walls",
        map_layout.lower_walls.len(),
        map_layout.roofs.len(),
        map_layout.ramps.len(),
        map_layout.roof_walls.len()
    );

    for wall in &map_layout.lower_walls {
        spawn_wall(&mut commands, &mut meshes, &mut materials, &asset_server, wall, debug_colors.0);
    }

    for light in &map_layout.wall_lights {
        spawn_wall_light_from_layout(&mut commands, &asset_server, light);
    }

    for roof in &map_layout.roofs {
        spawn_roof(&mut commands, &mut meshes, &mut materials, &asset_server, roof, debug_colors.0);
    }

    for ramp in &map_layout.ramps {
        spawn_ramp(&mut commands, &mut meshes, &mut materials, &asset_server, ramp);
    }

    for roof_wall in &map_layout.roof_walls {
        spawn_roof_wall(&mut commands, &mut meshes, &mut materials, roof_wall, debug_colors.0);
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
#[allow(clippy::implicit_hasher)]
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
