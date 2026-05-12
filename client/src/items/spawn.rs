use bevy::prelude::*;
use rand::random;

use crate::{
    barriers::BarrierAssets,
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

// Marker for client-spawned key entities. The rotation system queries this.
#[derive(Component)]
pub struct KeyMarker;

// Per-key elapsed time for the slow Y-axis spin. Independent phase per entity
// (set at spawn) so multiple keys near each other don't rotate in lockstep.
#[derive(Component)]
pub struct KeyRotationTimer(pub f32);

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
    // Keys share one mesh across all colors; their materials are not stored
    // here — they reuse the existing `BarrierAssets` per-color handles so the
    // pulsation is in sync with the matching barriers.
    key_mesh: Handle<Mesh>,
}

impl ItemAssets {
    fn power_up_material(&self, item_type: ItemType) -> &Handle<StandardMaterial> {
        match item_type {
            ItemType::SpeedPowerUp => &self.speed_material,
            ItemType::MultiShotPowerUp => &self.multishot_material,
            ItemType::PhasingPowerUp => &self.phasing_material,
            ItemType::AntiGravityPowerUp => &self.anti_gravity_material,
            ItemType::Cookie => unreachable!("cookies use cookie_material"),
            ItemType::Key(_) => unreachable!("keys use BarrierAssets materials"),
        }
    }

    pub fn key_mesh(&self) -> &Handle<Mesh> {
        &self.key_mesh
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
        key_mesh: meshes.add(Cuboid::new(KEY_WIDTH, KEY_HEIGHT, KEY_DEPTH)),
    });
}

// ============================================================================
// Item Spawning
// ============================================================================

// Get the color for an item type. Only meaningful for power-ups; cookies are
// white and keys are looked up from the config-driven `BarrierAssets` directly,
// so they aren't asked for here. Panics if a `Key` slips through.
#[must_use]
pub fn item_type_color(item_type: ItemType) -> Color {
    match item_type {
        ItemType::SpeedPowerUp => ITEM_SPEED_COLOR,
        ItemType::MultiShotPowerUp => ITEM_MULTISHOT_COLOR,
        ItemType::PhasingPowerUp => ITEM_PHASING_COLOR,
        ItemType::AntiGravityPowerUp => ITEM_ANTI_GRAVITY_COLOR,
        ItemType::Cookie => Color::WHITE,
        ItemType::Key(_) => unreachable!("keys look up colors via BarrierAssets / AssetSet, not item_type_color"),
    }
}

// Spawn an item cube
pub fn spawn_item(
    commands: &mut Commands,
    item_assets: &ItemAssets,
    barrier_assets: &BarrierAssets,
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

    // Keys: a small rotating cuboid that reuses the matching barrier
    // material — the glow + pulsation come for free.
    if let ItemType::Key(kind) = item_type {
        let random_phase = random::<f32>() * std::f32::consts::TAU;
        return commands
            .spawn((
                ItemBundle {
                    item_id,
                    item_marker: ItemMarker,
                    position: *position,
                    mesh: Mesh3d(item_assets.key_mesh.clone()),
                    material: MeshMaterial3d(barrier_assets.material_for(kind).clone()),
                    transform: Transform::from_xyz(
                        position.x,
                        position.y + KEY_HEIGHT_ABOVE_FLOOR,
                        position.z,
                    ),
                    visibility: Visibility::Visible,
                },
                level,
                KeyMarker,
                KeyRotationTimer(random_phase),
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
