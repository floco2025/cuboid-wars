use bevy::prelude::*;
use rand::random;
use std::f32::consts::{FRAC_PI_4, TAU};

use crate::{
    barriers::BarrierAssets,
    config::{AssetSet, ClientSettings},
    constants::*,
    items::{YSpinBase, YSpinTimer},
    map::MapLevel,
    missiles::{MissileAssets, spawn_missile_pickup_visual},
};
use common::{map::level_for_y, protocol::*};

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
    power_ups: Vec<PowerUpVisual>,
}

// One row per power-up kind: silhouette mesh, material, and the fixed "up"
// orientation the Y-spin stacks on. Adding a power-up means adding one row
// here instead of touching parallel matches.
struct PowerUpVisual {
    item_type: ItemType,
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
    base_orientation: Quat,
}

impl ItemAssets {
    fn power_up(&self, item_type: ItemType) -> &PowerUpVisual {
        self.power_ups
            .iter()
            .find(|visual| visual.item_type == item_type)
            .expect("item type missing from the power-up visual table")
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
        client_settings.rendering.mipmaps,
    ));

    let mut build_power_up = |item_type: ItemType| -> Handle<StandardMaterial> {
        let def = asset_set.material_for_item(item_type);
        materials.add(def.standard_item_material(
            &asset_server,
            item_type_color(item_type),
            client_settings.rendering.texture_anisotropy,
            client_settings.rendering.mipmaps,
        ))
    };
    let cuboid_mesh = meshes.add(Cuboid::new(ITEM_SIZE, ITEM_SIZE, ITEM_SIZE));

    // The map editor mirrors these silhouettes as 2D glyphs
    // (tools/map_editor/canvas.py `_paint_items`) — keep the two in sync.
    let power_ups = vec![
        // Speed: tetrahedron — angular silhouette reads as "fast". Default
        // Bevy `Tetrahedron` has two vertices at +y and two at -y (an edge
        // up); rotate so the vertex at (1,1,1) is the apex.
        PowerUpVisual {
            item_type: ItemType::SpeedPowerUp,
            mesh: meshes.add(Tetrahedron::default().mesh().build().scaled_by(Vec3::splat(ITEM_SIZE))),
            material: build_power_up(ItemType::SpeedPowerUp),
            base_orientation: Quat::from_rotation_arc(Vec3::new(1.0, 1.0, 1.0).normalize(), Vec3::Y),
        },
        // MultiShot keeps the original cube.
        PowerUpVisual {
            item_type: ItemType::MultiShotPowerUp,
            mesh: cuboid_mesh,
            material: build_power_up(ItemType::MultiShotPowerUp),
            base_orientation: Quat::IDENTITY,
        },
        // LowGravity: sphere — floats like an orb.
        PowerUpVisual {
            item_type: ItemType::LowGravityPowerUp,
            mesh: meshes.add(Sphere::new(ITEM_SIZE * 0.5)),
            material: build_power_up(ItemType::LowGravityPowerUp),
            base_orientation: Quat::IDENTITY,
        },
        // HealthPotion: vertical capsule tilted 45° around Z — vial / potion
        // silhouette leaning like a held bottle.
        PowerUpVisual {
            item_type: ItemType::HealthPotion,
            mesh: meshes.add(Capsule3d::new(ITEM_SIZE * 0.3, ITEM_SIZE)),
            material: build_power_up(ItemType::HealthPotion),
            base_orientation: Quat::from_rotation_z(FRAC_PI_4),
        },
    ];

    commands.insert_resource(ItemAssets {
        cookie_mesh: meshes.add(Sphere::new(COOKIE_SIZE)),
        cookie_material,
        power_ups,
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
        ItemType::LowGravityPowerUp => ITEM_LOW_GRAVITY_COLOR,
        ItemType::HealthPotion => ITEM_HEALTH_COLOR,
        ItemType::Cookie => Color::WHITE,
        ItemType::MissilePack => ITEM_MISSILE_COLOR,
        ItemType::Key(_) => unreachable!("keys look up colors via BarrierAssets / AssetSet, not item_type_color"),
    }
}

// Spawn an item cube
pub fn spawn_item(
    commands: &mut Commands,
    item_assets: &ItemAssets,
    barrier_assets: &BarrierAssets,
    missile_assets: &MissileAssets,
    item_id: ItemId,
    item_type: ItemType,
    position: &Position,
) -> Entity {
    let level = MapLevel(level_for_y(position.y));
    match item_type {
        ItemType::Cookie => spawn_cookie(commands, item_assets, item_id, position, level),
        ItemType::Key(kind) => spawn_key(commands, barrier_assets, item_id, position, level, kind),
        ItemType::MissilePack => spawn_missile_pack(commands, missile_assets, item_id, position, level),
        ItemType::SpeedPowerUp | ItemType::MultiShotPowerUp | ItemType::LowGravityPowerUp | ItemType::HealthPotion => {
            spawn_power_up(commands, item_assets, item_id, item_type, position, level)
        }
    }
}

fn spawn_missile_pack(
    commands: &mut Commands,
    missile_assets: &MissileAssets,
    item_id: ItemId,
    position: &Position,
    level: MapLevel,
) -> Entity {
    // The pickup IS a small missile: the flight meshes as children of a
    // bobbing, spinning item root, tilted like the potion so the silhouette
    // reads at a glance.
    let bob_phase = random::<f32>() * TAU;
    let spin_phase = random::<f32>() * TAU;
    let base = Quat::from_rotation_z(FRAC_PI_4);
    commands
        .spawn((
            item_id,
            ItemMarker,
            *position,
            Transform::from_xyz(
                position.x,
                position.y + ITEM_HEIGHT_ABOVE_FLOOR + ITEM_SIZE / 2.0,
                position.z,
            )
            .with_rotation(base),
            Visibility::Visible,
            level,
            ItemAnimTimer(bob_phase),
            YSpinTimer(spin_phase),
            YSpinBase(base),
        ))
        .with_children(|parent| {
            spawn_missile_pickup_visual(parent, missile_assets);
        })
        .id()
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
    let random_phase = random::<f32>() * TAU;
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
    // doesn't move in lockstep. The base orientation per shape is baked into
    // the spin's start rotation; the rotation system composes
    // `Quat::from_rotation_y(spin) * base`.
    let bob_phase = random::<f32>() * TAU;
    let spin_phase = random::<f32>() * TAU;
    let visual = item_assets.power_up(item_type);
    let base = visual.base_orientation;
    commands
        .spawn((
            ItemBundle {
                item_id,
                item_marker: ItemMarker,
                position: *position,
                mesh: Mesh3d(visual.mesh.clone()),
                material: MeshMaterial3d(visual.material.clone()),
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
