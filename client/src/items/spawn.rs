use bevy::prelude::*;
use rand::random;

use crate::{
    barriers::BarrierAssets,
    config::{AssetSet, ClientSettings},
    constants::*,
    items::y_spin::{YSpinBase, YSpinTimer},
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
// Cookies + power-ups. Keys live entirely on `BarrierAssets` because they
// share materials + a thematic family with barriers.
#[derive(Resource)]
pub struct ItemAssets {
    cookie_mesh: Handle<Mesh>,
    cookie_material: Handle<StandardMaterial>,
    speed_mesh: Handle<Mesh>,
    multishot_mesh: Handle<Mesh>,
    phasing_mesh: Handle<Mesh>,
    low_gravity_mesh: Handle<Mesh>,
    health_mesh: Handle<Mesh>,
    speed_material: Handle<StandardMaterial>,
    multishot_material: Handle<StandardMaterial>,
    phasing_material: Handle<StandardMaterial>,
    low_gravity_material: Handle<StandardMaterial>,
    health_material: Handle<StandardMaterial>,
}

impl ItemAssets {
    fn power_up_material(&self, item_type: ItemType) -> &Handle<StandardMaterial> {
        match item_type {
            ItemType::SpeedPowerUp => &self.speed_material,
            ItemType::MultiShotPowerUp => &self.multishot_material,
            ItemType::PhasingPowerUp => &self.phasing_material,
            ItemType::LowGravityPowerUp => &self.low_gravity_material,
            ItemType::HealthPotion => &self.health_material,
            ItemType::Cookie => unreachable!("cookies use cookie_material"),
            ItemType::Key(_) => unreachable!("keys use BarrierAssets"),
        }
    }

    fn power_up_mesh(&self, item_type: ItemType) -> &Handle<Mesh> {
        match item_type {
            ItemType::SpeedPowerUp => &self.speed_mesh,
            ItemType::MultiShotPowerUp => &self.multishot_mesh,
            ItemType::PhasingPowerUp => &self.phasing_mesh,
            ItemType::LowGravityPowerUp => &self.low_gravity_mesh,
            ItemType::HealthPotion => &self.health_mesh,
            ItemType::Cookie => unreachable!("cookies use cookie_mesh"),
            ItemType::Key(_) => unreachable!("keys use BarrierAssets"),
        }
    }
}

pub fn setup_item_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
    asset_set: Res<AssetSet>,
    client_settings: Res<ClientSettings>,
) {
    let cookie_def = asset_set.material_for_item(ItemType::Cookie);
    let cookie_material = materials.add(cookie_def.standard_material(
        &asset_server,
        client_settings.rendering.texture_anisotropy,
        client_settings.rendering.texture_mipmaps_enabled,
    ));

    let mut build_power_up = |item_type: ItemType| -> Handle<StandardMaterial> {
        let def = asset_set.material_for_item(item_type);
        materials.add(def.standard_item_material(
            &asset_server,
            item_type_color(item_type),
            client_settings.rendering.texture_anisotropy,
            client_settings.rendering.texture_mipmaps_enabled,
        ))
    };
    let speed_material = build_power_up(ItemType::SpeedPowerUp);
    let multishot_material = build_power_up(ItemType::MultiShotPowerUp);
    let phasing_material = build_power_up(ItemType::PhasingPowerUp);
    let low_gravity_material = build_power_up(ItemType::LowGravityPowerUp);
    let health_material = build_power_up(ItemType::HealthPotion);

    let cuboid_mesh = meshes.add(Cuboid::new(ITEM_SIZE, ITEM_SIZE, ITEM_SIZE));

    // The map editor mirrors these silhouettes as 2D glyphs
    // (tools/map_editor/canvas.py `_paint_items`) — keep the two in sync.
    commands.insert_resource(ItemAssets {
        cookie_mesh: meshes.add(Sphere::new(COOKIE_SIZE)),
        cookie_material,
        // Speed: tetrahedron — angular silhouette reads as "fast".
        speed_mesh: meshes.add(Tetrahedron::default().mesh().build().scaled_by(Vec3::splat(ITEM_SIZE))),
        // MultiShot + Phasing keep the original cube.
        multishot_mesh: cuboid_mesh.clone(),
        phasing_mesh: cuboid_mesh,
        // LowGravity: sphere — floats like an orb.
        low_gravity_mesh: meshes.add(Sphere::new(ITEM_SIZE * 0.5)),
        // HealthPotion: vertical capsule — vial / potion silhouette.
        health_mesh: meshes.add(Capsule3d::new(ITEM_SIZE * 0.3, ITEM_SIZE)),
        speed_material,
        multishot_material,
        phasing_material,
        low_gravity_material,
        health_material,
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
        ItemType::LowGravityPowerUp => ITEM_LOW_GRAVITY_COLOR,
        ItemType::HealthPotion => ITEM_HEALTH_COLOR,
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
    match item_type {
        ItemType::Cookie => spawn_cookie(commands, item_assets, item_id, position, level),
        ItemType::Key(kind) => spawn_key(commands, barrier_assets, item_id, position, level, kind),
        ItemType::SpeedPowerUp
        | ItemType::MultiShotPowerUp
        | ItemType::PhasingPowerUp
        | ItemType::LowGravityPowerUp
        | ItemType::HealthPotion => spawn_power_up(commands, item_assets, item_id, item_type, position, level),
    }
}

fn spawn_cookie(
    commands: &mut Commands,
    item_assets: &ItemAssets,
    item_id: ItemId,
    position: &Position,
    level: MapLevel,
) -> Entity {
    // Cookies are small textured spheres on the floor; no animation.
    commands
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
        .id()
}

fn spawn_key(
    commands: &mut Commands,
    barrier_assets: &BarrierAssets,
    item_id: ItemId,
    position: &Position,
    level: MapLevel,
    kind: common::protocol::BarrierKindId,
) -> Entity {
    // Keys are a small rotating cuboid that reuses the matching barrier
    // material, so the pulse stays in sync. Per-instance random phase keeps
    // multiple nearby keys from rotating in lockstep.
    let random_phase = random::<f32>() * std::f32::consts::TAU;
    commands
        .spawn((
            ItemBundle {
                item_id,
                item_marker: ItemMarker,
                position: *position,
                mesh: Mesh3d(barrier_assets.key_mesh().clone()),
                material: MeshMaterial3d(barrier_assets.material_for(kind).clone()),
                transform: Transform::from_xyz(position.x, position.y + KEY_HEIGHT_ABOVE_FLOOR, position.z),
                visibility: Visibility::Visible,
            },
            level,
            YSpinTimer(random_phase),
        ))
        .id()
}

fn spawn_power_up(
    commands: &mut Commands,
    item_assets: &ItemAssets,
    item_id: ItemId,
    item_type: ItemType,
    position: &Position,
    level: MapLevel,
) -> Entity {
    // Power-ups bob up and down (translation) and spin around Y (rotation),
    // each driven by independent per-instance random phases so a cluster
    // doesn't move in lockstep. The base orientation per shape (apex-up
    // tetrahedron, tilted capsule, identity for cube/sphere) is baked into
    // the spin's start rotation; the rotation system composes
    // `Quat::from_rotation_y(spin) * base`.
    let bob_phase = random::<f32>() * std::f32::consts::TAU;
    let spin_phase = random::<f32>() * std::f32::consts::TAU;
    let base = power_up_base_orientation(item_type);
    commands
        .spawn((
            ItemBundle {
                item_id,
                item_marker: ItemMarker,
                position: *position,
                mesh: Mesh3d(item_assets.power_up_mesh(item_type).clone()),
                material: MeshMaterial3d(item_assets.power_up_material(item_type).clone()),
                transform: Transform::from_xyz(
                    position.x,
                    position.y + ITEM_HEIGHT_ABOVE_FLOOR + ITEM_SIZE / 2.0,
                    position.z,
                )
                .with_rotation(base),
                visibility: Visibility::Visible,
            },
            level,
            ItemAnimTimer(bob_phase),
            YSpinTimer(spin_phase),
            YSpinBase(base),
        ))
        .id()
}

// Per-shape fixed "up" orientation. The spin around Y stacks on top so the
// chosen up direction stays constant while the mesh rotates.
fn power_up_base_orientation(item_type: ItemType) -> Quat {
    match item_type {
        // Default Bevy `Tetrahedron` has two vertices at +y and two at -y
        // (an edge up). Rotate so the vertex at (1,1,1) is the apex.
        ItemType::SpeedPowerUp => Quat::from_rotation_arc(Vec3::new(1.0, 1.0, 1.0).normalize(), Vec3::Y),
        // 45° tilt around Z — capsule leans like a held potion.
        ItemType::HealthPotion => Quat::from_rotation_z(std::f32::consts::FRAC_PI_4),
        ItemType::MultiShotPowerUp | ItemType::PhasingPowerUp | ItemType::LowGravityPowerUp => Quat::IDENTITY,
        ItemType::Cookie => unreachable!("cookies don't spawn via spawn_power_up"),
        ItemType::Key(_) => unreachable!("keys don't spawn via spawn_power_up"),
    }
}
