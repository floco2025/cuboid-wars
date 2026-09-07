use bevy::{light::NotShadowCaster, prelude::*};

use crate::{
    carriers::CarrierEntities,
    constants::{PORTAL_A_COLOR, PORTAL_B_COLOR, PORTAL_EMISSIVE, PORTAL_FIZZLE_LIFETIME, PORTAL_FIZZLE_SPARK_SIZE},
};
use common::{
    physics::PortalFrame,
    protocol::{Portal, PortalEnd},
};

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
        let ring = meshes.add(Annulus::new(0.84, 1.0));
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
    let center = frame.center + frame.normal * 0.025;
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
        Vec3::new(0.32, 0.56, 1.0),
        Vec3::ZERO,
        PORTAL_FIZZLE_LIFETIME,
        true,
    );
    spawn(
        assets.flash.clone(),
        center + frame.normal * 0.003,
        Vec3::new(0.23, 0.4, 1.0),
        Vec3::ZERO,
        0.12,
        false,
    );
    for index in 0..12 {
        let angle = index as f32 * std::f32::consts::TAU / 12.0;
        let radial = frame.right * angle.cos() + frame.up * angle.sin();
        let velocity = radial * (0.7 + 0.1 * (index % 3) as f32) + frame.normal * 0.5;
        spawn(
            assets.spark.clone(),
            center + radial * 0.07,
            Vec3::splat(PORTAL_FIZZLE_SPARK_SIZE),
            velocity,
            PORTAL_FIZZLE_LIFETIME * (0.65 + 0.03 * index as f32),
            false,
        );
    }
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
        let size = if effect.ring && progress < 0.15 {
            0.3 + progress / 0.15 * 0.7
        } else if effect.ring {
            ((1.0 - progress) / 0.85).powi(2)
        } else {
            1.0 - progress
        };
        transform.scale = effect.scale * size;
        transform.translation += effect.velocity * time.delta_secs();
    }
}
