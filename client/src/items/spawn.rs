use bevy::prelude::*;
use rand::random;

use crate::{
    config::{AssetSet, RenderSettings},
    constants::*,
    map::MapLevel,
};
use common::{map::compute_player_level, protocol::*};

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
    visibility: Visibility,
}

// ============================================================================
// Shared item assets
// ============================================================================

// One mesh + one material per item kind, built at startup and cloned cheaply
// into every spawned item. Sharing handles lets Bevy's automatic batching
// collapse N item draws into a handful of instanced calls — without this,
// hundreds of cookies each force their own draw call.
#[derive(Resource)]
pub struct ItemAssets {
    cookie_mesh: Handle<Mesh>,
    cookie_material: Handle<StandardMaterial>,
    power_up_mesh: Handle<Mesh>,
    speed_material: Handle<StandardMaterial>,
    multishot_material: Handle<StandardMaterial>,
    phasing_material: Handle<StandardMaterial>,
    anti_gravity_material: Handle<StandardMaterial>,
}

impl ItemAssets {
    fn power_up_material(&self, item_type: ItemType) -> &Handle<StandardMaterial> {
        match item_type {
            ItemType::SpeedPowerUp => &self.speed_material,
            ItemType::MultiShotPowerUp => &self.multishot_material,
            ItemType::PhasingPowerUp => &self.phasing_material,
            ItemType::AntiGravityPowerUp => &self.anti_gravity_material,
            ItemType::Cookie => unreachable!("cookies use cookie_material"),
        }
    }
}

pub fn setup_item_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
    asset_set: Res<AssetSet>,
    render_settings: Res<RenderSettings>,
) {
    let cookie_def = asset_set.material_for_item(ItemType::Cookie);
    let cookie_material = materials.add(cookie_def.standard_material(
        &asset_server,
        render_settings.texture_anisotropy,
        render_settings.texture_mipmaps_enabled,
    ));

    let mut build_power_up = |item_type: ItemType| -> Handle<StandardMaterial> {
        let def = asset_set.material_for_item(item_type);
        materials.add(def.standard_item_material(
            &asset_server,
            item_type_color(item_type),
            render_settings.texture_anisotropy,
            render_settings.texture_mipmaps_enabled,
        ))
    };
    let speed_material = build_power_up(ItemType::SpeedPowerUp);
    let multishot_material = build_power_up(ItemType::MultiShotPowerUp);
    let phasing_material = build_power_up(ItemType::PhasingPowerUp);
    let anti_gravity_material = build_power_up(ItemType::AntiGravityPowerUp);

    commands.insert_resource(ItemAssets {
        cookie_mesh: meshes.add(Sphere::new(COOKIE_SIZE)),
        cookie_material,
        power_up_mesh: meshes.add(Cuboid::new(ITEM_SIZE, ITEM_SIZE, ITEM_SIZE)),
        speed_material,
        multishot_material,
        phasing_material,
        anti_gravity_material,
    });
}

// ============================================================================
// Item Spawning
// ============================================================================

// Get the color for an item type
#[must_use]
pub const fn item_type_color(item_type: ItemType) -> Color {
    match item_type {
        ItemType::SpeedPowerUp => ITEM_SPEED_COLOR,
        ItemType::MultiShotPowerUp => ITEM_MULTISHOT_COLOR,
        ItemType::PhasingPowerUp => ITEM_PHASING_COLOR,
        ItemType::AntiGravityPowerUp => ITEM_ANTI_GRAVITY_COLOR,
        ItemType::Cookie => Color::WHITE,
    }
}

// Spawn an item cube
pub fn spawn_item(
    commands: &mut Commands,
    item_assets: &ItemAssets,
    item_id: ItemId,
    item_type: ItemType,
    position: &Position,
) -> Entity {
    let level = MapLevel(compute_player_level(position.y));

    // Cookies are rendered differently - small spheres on the floor with textures
    if item_type == ItemType::Cookie {
        return commands
            .spawn((
                ItemBundle {
                    item_id,
                    item_marker: ItemMarker,
                    position: *position,
                    mesh: Mesh3d(item_assets.cookie_mesh.clone()),
                    material: MeshMaterial3d(item_assets.cookie_material.clone()),
                    transform: Transform::from_xyz(position.x, position.y + COOKIE_HEIGHT, position.z),
                    visibility: Visibility::Visible,
                },
                level,
            ))
            .id();
    }

    // Power-ups are cubes that bounce with textured materials
    let random_phase = random::<f32>() * std::f32::consts::TAU;

    commands
        .spawn((
            ItemBundle {
                item_id,
                item_marker: ItemMarker,
                position: *position,
                mesh: Mesh3d(item_assets.power_up_mesh.clone()),
                material: MeshMaterial3d(item_assets.power_up_material(item_type).clone()),
                transform: Transform::from_xyz(
                    position.x,
                    position.y + ITEM_HEIGHT_ABOVE_FLOOR + ITEM_SIZE / 2.0,
                    position.z,
                ),
                visibility: Visibility::Visible,
            },
            level,
            ItemAnimTimer(random_phase),
        ))
        .id()
}
