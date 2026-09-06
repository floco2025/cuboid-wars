use bevy::prelude::*;
use rand::random;
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, TAU};

use crate::{
    barriers::BarrierAssets,
    config::ClientSettings,
    constants::*,
    items::{CoinAssets, YSpinBase, YSpinTimer, item_symbol_mesh, pickup_material, spawn_coin_visual},
    map::MapLevel,
    missiles::{MissileAssets, spawn_missile_pickup_visual},
};
use common::{
    constants::{PORTAL_HALF_HEIGHT, PORTAL_HALF_WIDTH},
    protocol::*,
};

// ============================================================================
// Components
// ============================================================================

#[derive(Component)]
pub struct ItemAnimTimer(pub f32);

// ============================================================================
// Shared item assets
// ============================================================================

// Shared mesh and material handles let Bevy batch repeated pickups.
// Keys live on `BarrierAssets` because their colors come from barrier kinds.
#[derive(Resource)]
pub struct ItemAssets {
    coin: CoinAssets,
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
    client_settings: Res<ClientSettings>,
) {
    let glow = client_settings.vfx.pickup_emissive_brightness;
    let coin = CoinAssets::new(&mut meshes, &mut materials, glow);
    let mut build_power_up = |item_type: ItemType| -> Handle<StandardMaterial> {
        materials.add(pickup_material(item_type_color(item_type), glow))
    };
    let power_ups = vec![
        PowerUpVisual {
            item_type: ItemType::PortalGunPowerUp,
            mesh: meshes.add(
                Torus {
                    minor_radius: ITEM_SIZE * 0.07,
                    major_radius: ITEM_SIZE * 0.43,
                }
                .mesh()
                .build()
                .scaled_by(Vec3::new(1.0, 1.0, PORTAL_HALF_HEIGHT / PORTAL_HALF_WIDTH)),
            ),
            material: build_power_up(ItemType::PortalGunPowerUp),
            base_orientation: Quat::from_rotation_x(FRAC_PI_2),
        },
        PowerUpVisual {
            item_type: ItemType::SpeedPowerUp,
            mesh: meshes.add(item_symbol_mesh(
                ItemType::SpeedPowerUp,
                ITEM_SIZE * 1.5,
                ITEM_SIZE * 0.24,
            )),
            material: build_power_up(ItemType::SpeedPowerUp),
            base_orientation: Quat::IDENTITY,
        },
        PowerUpVisual {
            item_type: ItemType::MultiShotPowerUp,
            mesh: meshes.add(item_symbol_mesh(
                ItemType::MultiShotPowerUp,
                ITEM_SIZE * 1.5,
                ITEM_SIZE * 0.24,
            )),
            material: build_power_up(ItemType::MultiShotPowerUp),
            base_orientation: Quat::IDENTITY,
        },
        PowerUpVisual {
            item_type: ItemType::LowGravityPowerUp,
            mesh: meshes.add(item_symbol_mesh(
                ItemType::LowGravityPowerUp,
                ITEM_SIZE * 1.5,
                ITEM_SIZE * 0.24,
            )),
            material: build_power_up(ItemType::LowGravityPowerUp),
            base_orientation: Quat::IDENTITY,
        },
        PowerUpVisual {
            item_type: ItemType::HealthPotion,
            mesh: meshes.add(item_symbol_mesh(
                ItemType::HealthPotion,
                ITEM_SIZE * 1.5,
                ITEM_SIZE * 0.24,
            )),
            material: build_power_up(ItemType::HealthPotion),
            base_orientation: Quat::IDENTITY,
        },
    ];

    commands.insert_resource(ItemAssets { coin, power_ups });
}

// ============================================================================
// Item Spawning
// ============================================================================

// Keys use their barrier kind’s color instead of a fixed item color.
#[must_use]
pub fn item_type_color(item_type: ItemType) -> Color {
    match item_type {
        ItemType::SpeedPowerUp => ITEM_SPEED_COLOR,
        ItemType::MultiShotPowerUp => ITEM_MULTISHOT_COLOR,
        ItemType::LowGravityPowerUp => ITEM_LOW_GRAVITY_COLOR,
        ItemType::PortalGunPowerUp => PORTAL_A_COLOR,
        ItemType::HealthPotion => ITEM_HEALTH_COLOR,
        ItemType::Cookie => ITEM_COIN_COLOR,
        ItemType::MissilePack => ITEM_MISSILE_COLOR,
        ItemType::Key(_) => unreachable!("keys look up colors via BarrierAssets / AssetSet, not item_type_color"),
    }
}

// Spawn the item in its carrier’s frame.
pub fn spawn_item(
    commands: &mut Commands,
    item_assets: &ItemAssets,
    barrier_assets: &BarrierAssets,
    missile_assets: &MissileAssets,
    carrier: Entity,
    level: MapLevel,
    item_id: ItemId,
    item_type: ItemType,
    position: &Position,
) -> Entity {
    let spin_phase = random::<f32>() * TAU;
    let mut entity = commands.spawn((
        item_id,
        ItemMarker,
        *position,
        Visibility::Visible,
        level,
        ChildOf(carrier),
        ItemAnimTimer(random::<f32>() * TAU),
        YSpinTimer(spin_phase),
    ));
    let base = match item_type {
        ItemType::Cookie => {
            entity.with_children(|parent| spawn_coin_visual(parent, &item_assets.coin));
            Quat::IDENTITY
        }
        ItemType::Key(kind) => {
            entity.insert((
                Mesh3d(barrier_assets.key_mesh().clone()),
                MeshMaterial3d(barrier_assets.key_material_for(kind).clone()),
            ));
            Quat::IDENTITY
        }
        ItemType::MissilePack => {
            entity.with_children(|parent| {
                spawn_missile_pickup_visual(parent, missile_assets);
            });
            Quat::from_rotation_z(FRAC_PI_4)
        }
        ItemType::SpeedPowerUp
        | ItemType::MultiShotPowerUp
        | ItemType::LowGravityPowerUp
        | ItemType::HealthPotion
        | ItemType::PortalGunPowerUp => {
            let visual = item_assets.power_up(item_type);
            entity.insert((Mesh3d(visual.mesh.clone()), MeshMaterial3d(visual.material.clone())));
            visual.base_orientation
        }
    };
    entity
        .insert((
            Transform::from_xyz(position.x, position.y + ITEM_HEIGHT_ABOVE_FLOOR, position.z)
                .with_rotation(Quat::from_rotation_y(spin_phase) * base),
            YSpinBase(base),
        ))
        .id()
}
