use bevy::{gltf::GltfAssetLabel, prelude::*, scene::SceneRoot};

use crate::constants::*;
use common::{markers::ItemMarker, protocol::*};

// ============================================================================
// Components
// ============================================================================

#[derive(Component)]
pub struct ItemAnimTimer(pub f32);

// ============================================================================
// Bundles
// ============================================================================

#[derive(Bundle)]
struct ItemBundle {
    item_id: ItemId,
    item_marker: ItemMarker,
    position: Position,
    mesh: Mesh3d,
    material: MeshMaterial3d<StandardMaterial>,
    transform: Transform,
}

// ============================================================================
// Item Spawning
// ============================================================================

// Get the color for an item type
#[must_use]
pub const fn item_type_color(item_type: ItemType) -> Color {
    match item_type {
        ItemType::SpeedPowerUp => Color::srgb(ITEM_SPEED_COLOR[0], ITEM_SPEED_COLOR[1], ITEM_SPEED_COLOR[2]),
        ItemType::MultiShotPowerUp => Color::srgb(
            ITEM_MULTISHOT_COLOR[0],
            ITEM_MULTISHOT_COLOR[1],
            ITEM_MULTISHOT_COLOR[2],
        ),
        ItemType::PhasingPowerUp => Color::srgb(ITEM_PHASING_COLOR[0], ITEM_PHASING_COLOR[1], ITEM_PHASING_COLOR[2]),
        ItemType::SentryHunterPowerUp => Color::srgb(
            ITEM_SENTRY_HUNT_COLOR[0],
            ITEM_SENTRY_HUNT_COLOR[1],
            ITEM_SENTRY_HUNT_COLOR[2],
        ),
        ItemType::Cookie => Color::WHITE, // Cookies use textures, not colors
    }
}

// Spawn an item cube
pub fn spawn_item(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    asset_server: &Res<AssetServer>,
    item_id: ItemId,
    item_type: ItemType,
    position: &Position,
) -> Entity {
    // Cookies are rendered differently - small spheres on the floor with textures
    if item_type == ItemType::Cookie {
        return commands
            .spawn(ItemBundle {
                item_id,
                item_marker: ItemMarker,
                position: *position,
                mesh: Mesh3d(meshes.add(Sphere::new(COOKIE_SIZE))),
                material: MeshMaterial3d(materials.add(StandardMaterial {
                    base_color_texture: Some(asset_server.load("textures/cookie/albedo.png")),
                    normal_map_texture: Some(asset_server.load("textures/cookie/normal-dx.png")),
                    occlusion_texture: Some(asset_server.load("textures/cookie/ao.png")),
                    metallic_roughness_texture: Some(asset_server.load("textures/cookie/metallic-roughness.png")),
                    metallic: TEXTURE_COOKIE_METALLIC,
                    perceptual_roughness: TEXTURE_COOKIE_ROUGHNESS,
                    ..default()
                })),
                transform: Transform::from_xyz(position.x, COOKIE_HEIGHT, position.z),
            })
            .id();
    }

    // Power-ups are cubes that bounce with textured materials
    let random_phase = rand::random::<f32>() * std::f32::consts::TAU;

    commands
        .spawn((
            ItemBundle {
                item_id,
                item_marker: ItemMarker,
                position: *position,
                mesh: Mesh3d(meshes.add(Cuboid::new(ITEM_SIZE, ITEM_SIZE, ITEM_SIZE))),
                material: MeshMaterial3d(materials.add(StandardMaterial {
                    base_color_texture: Some(asset_server.load("textures/item/albedo.png")),
                    normal_map_texture: Some(asset_server.load("textures/item/normal-dx.png")),
                    occlusion_texture: Some(asset_server.load("textures/item/ao.png")),
                    metallic_roughness_texture: Some(asset_server.load("textures/item/metallic-roughness.png")),
                    base_color: item_type_color(item_type),
                    emissive: LinearRgba::from(item_type_color(item_type)) * ITEM_EMISSIVE_STRENGTH,
                    metallic: TEXTURE_ITEM_METALLIC,
                    perceptual_roughness: TEXTURE_ITEM_ROUGHNESS,
                    ..default()
                })),
                transform: Transform::from_xyz(position.x, ITEM_HEIGHT_ABOVE_FLOOR + ITEM_SIZE / 2.0, position.z),
            },
            ItemAnimTimer(random_phase),
        ))
        .id()
}

// Spawn a wall light from precomputed layout data (world-space position and yaw).
pub fn spawn_wall_light_from_layout(commands: &mut Commands, asset_server: &Res<AssetServer>, light: &WallLight) {
    let light_scene: Handle<Scene> = asset_server.load(GltfAssetLabel::Scene(0).from_asset(WALL_LIGHT_MODEL));

    let model_yaw = Quat::from_rotation_y(light.yaw);
    let (sin_yaw, cos_yaw) = light.yaw.sin_cos();
    let light_pos = Vec3::new(
        WALL_LIGHT_INWARD_OFFSET.mul_add(sin_yaw, light.pos.x),
        light.pos.y,
        WALL_LIGHT_INWARD_OFFSET.mul_add(cos_yaw, light.pos.z),
    );

    commands.spawn((
        SceneRoot(light_scene),
        Transform::from_xyz(light.pos.x, light.pos.y, light.pos.z)
            .with_scale(Vec3::splat(WALL_LIGHT_SCALE))
            .with_rotation(model_yaw),
        GlobalTransform::default(),
        Visibility::Visible,
        InheritedVisibility::default(),
        ViewVisibility::default(),
    ));

    commands.spawn((
        PointLight {
            intensity: WALL_LIGHT_BRIGHTNESS,
            range: WALL_LIGHT_RANGE,
            radius: WALL_LIGHT_RADIUS,
            shadows_enabled: false,
            color: Color::srgb(1.0, 0.95, 0.85),
            ..default()
        },
        Transform::from_xyz(light_pos.x, light_pos.y, light_pos.z),
    ));
}
