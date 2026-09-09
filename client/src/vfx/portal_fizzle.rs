use bevy::{light::NotShadowCaster, prelude::*};

use crate::{carriers::CarrierEntities, constants::*};
use common::{
    constants::{PORTAL_HALF_HEIGHT, PORTAL_HALF_WIDTH},
    physics::PortalFrame,
    protocol::{Portal, PortalEnd},
};

// Off the surface so the discs never z-fight with it, the flash a hair above the ring.
const SURFACE_LIFT: f32 = 0.025;
const FLASH_LIFT: f32 = 0.003;

#[derive(Resource)]
pub struct PortalFizzleAssets {
    ring: Handle<Mesh>,
    flash: Handle<Mesh>,
    spark: Handle<Mesh>,
    materials: [Handle<StandardMaterial>; 2],
}

impl FromWorld for PortalFizzleAssets {
    fn from_world(world: &mut World) -> Self {
        let mut meshes = world.resource_mut::<Assets<Mesh>>();
        let ring = meshes.add(Annulus::new(PORTAL_FIZZLE_RING_INNER_RADIUS, 1.0));
        let flash = meshes.add(Circle::new(1.0));
        let spark = meshes.add(Cuboid::from_length(1.0));
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
        let materials = [PORTAL_A_COLOR, PORTAL_B_COLOR].map(|color| {
            materials.add(StandardMaterial {
                base_color: color,
                emissive: color.to_linear() * PORTAL_EMISSIVE,
                unlit: true,
                ..default()
            })
        });
        Self {
            ring,
            flash,
            spark,
            materials,
        }
    }
}

#[derive(Component)]
pub struct PortalFizzle {
    elapsed: f32,
    lifetime: f32,
    scale: Vec3,
    velocity: Vec3,
    ring: bool,
}

pub fn spawn_portal_fizzle(
    commands: &mut Commands,
    assets: &PortalFizzleAssets,
    impact: &Portal,
    carriers: &CarrierEntities,
) {
    let frame = PortalFrame::from_surface(
        impact.pos.into(),
        Vec3::new(impact.nx, impact.ny, impact.nz),
        impact.yaw,
    );
    let rotation = Quat::from_mat3(&Mat3::from_cols(frame.right, frame.up, frame.normal));
    let center = frame.center + frame.normal * SURFACE_LIFT;
    let material = assets.materials[usize::from(impact.end == PortalEnd::B)].clone();
    let mut spawn = |mesh: Handle<Mesh>, position, scale, velocity, lifetime, ring| {
        commands.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(material.clone()),
            Transform::from_translation(position)
                .with_rotation(rotation)
                .with_scale(scale),
            ChildOf(carriers.get(impact.carrier)),
            NotShadowCaster,
            PortalFizzle {
                elapsed: 0.0,
                lifetime,
                scale,
                velocity,
                ring,
            },
        ));
    };
    spawn(
        assets.ring.clone(),
        center,
        aperture_scale(PORTAL_FIZZLE_RING_APERTURE_FRACTION),
        Vec3::ZERO,
        PORTAL_FIZZLE_LIFETIME,
        true,
    );
    spawn(
        assets.flash.clone(),
        center + frame.normal * FLASH_LIFT,
        aperture_scale(PORTAL_FIZZLE_FLASH_APERTURE_FRACTION),
        Vec3::ZERO,
        PORTAL_FIZZLE_FLASH_LIFETIME,
        false,
    );
    for index in 0..PORTAL_FIZZLE_SPARK_COUNT {
        let angle = index as f32 * std::f32::consts::TAU / PORTAL_FIZZLE_SPARK_COUNT as f32;
        let radial = frame.right * angle.cos() + frame.up * angle.sin();
        let radial_speed =
            PORTAL_FIZZLE_SPARK_RADIAL_SPEED + PORTAL_FIZZLE_SPARK_RADIAL_SPEED_STEP * (index % 3) as f32;
        spawn(
            assets.spark.clone(),
            center + radial * PORTAL_FIZZLE_SPARK_START_RADIUS,
            Vec3::splat(PORTAL_FIZZLE_SPARK_SIZE),
            radial * radial_speed + frame.normal * PORTAL_FIZZLE_SPARK_LIFT_SPEED,
            PORTAL_FIZZLE_LIFETIME
                * (PORTAL_FIZZLE_SPARK_LIFETIME_FRACTION + PORTAL_FIZZLE_SPARK_LIFETIME_STEP * index as f32),
            false,
        );
    }
}

// A disc's semi-axes at the given fraction of the aperture's.
fn aperture_scale(fraction: f32) -> Vec3 {
    Vec3::new(PORTAL_HALF_WIDTH * fraction, PORTAL_HALF_HEIGHT * fraction, 1.0)
}

pub fn portal_fizzle_system(
    mut commands: Commands,
    time: Res<Time>,
    mut effects: Query<(Entity, &mut PortalFizzle, &mut Transform)>,
) {
    for (entity, mut effect, mut transform) in &mut effects {
        effect.elapsed += time.delta_secs();
        let progress = effect.elapsed / effect.lifetime;
        if progress >= 1.0 {
            commands.entity(entity).despawn();
            continue;
        }
        let size = if effect.ring && progress < PORTAL_FIZZLE_RING_GROW_FRACTION {
            PORTAL_FIZZLE_RING_START_SCALE
                + progress / PORTAL_FIZZLE_RING_GROW_FRACTION * (1.0 - PORTAL_FIZZLE_RING_START_SCALE)
        } else if effect.ring {
            ((1.0 - progress) / (1.0 - PORTAL_FIZZLE_RING_GROW_FRACTION)).powi(2)
        } else {
            1.0 - progress
        };
        transform.scale = effect.scale * size;
        transform.translation += effect.velocity * time.delta_secs();
    }
}
