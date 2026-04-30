use bevy::prelude::*;
use rand::random;

use crate::{config::AssetSet, constants::*, markers::MapLevel};
use common::{map::compute_player_level, markers::ItemMarker, protocol::*};

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
// Item Spawning
// ============================================================================

// Get the color for an item type
#[must_use]
pub const fn item_type_color(item_type: ItemType) -> Color {
    match item_type {
        ItemType::SpeedPowerUp => ITEM_SPEED_COLOR,
        ItemType::MultiShotPowerUp => ITEM_MULTISHOT_COLOR,
        ItemType::PhasingPowerUp => ITEM_PHASING_COLOR,
        ItemType::Cookie => Color::WHITE,
    }
}

// Spawn an item cube
pub fn spawn_item(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    asset_server: &Res<AssetServer>,
    asset_set: &AssetSet,
    item_id: ItemId,
    item_type: ItemType,
    position: &Position,
) -> Entity {
    let level = MapLevel(compute_player_level(position.y));
    let material_def = asset_set.material_for_item(item_type);

    // Cookies are rendered differently - small spheres on the floor with textures
    if item_type == ItemType::Cookie {
        return commands
            .spawn((
                ItemBundle {
                    item_id,
                    item_marker: ItemMarker,
                    position: *position,
                    mesh: Mesh3d(meshes.add(Sphere::new(COOKIE_SIZE))),
                    material: MeshMaterial3d(materials.add(material_def.standard_material(asset_server))),
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
                mesh: Mesh3d(meshes.add(Cuboid::new(ITEM_SIZE, ITEM_SIZE, ITEM_SIZE))),
                material: MeshMaterial3d(
                    materials.add(material_def.standard_item_material(asset_server, item_type_color(item_type))),
                ),
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
