use bevy::{light::NotShadowCaster, prelude::*};
use rand::{RngExt, rng};

use crate::constants::{
    PROJECTILE_SPARK_COUNT, PROJECTILE_SPARK_GRAVITY, PROJECTILE_SPARK_LIFETIME_SECS, PROJECTILE_SPARK_SIZE,
    PROJECTILE_SPARK_SPEED,
};

#[derive(Resource)]
pub struct SparkAssets {
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
}

impl FromWorld for SparkAssets {
    fn from_world(world: &mut World) -> Self {
        let brightness = world
            .resource::<crate::config::ClientSettings>()
            .projectiles
            .spark_brightness;
        let mesh = world.resource_mut::<Assets<Mesh>>().add(Cuboid::new(
            PROJECTILE_SPARK_SIZE,
            PROJECTILE_SPARK_SIZE,
            PROJECTILE_SPARK_SIZE,
        ));
        let material = world.resource_mut::<Assets<StandardMaterial>>().add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.85, 0.3),
            emissive: LinearRgba::rgb(brightness, brightness * 0.8, brightness * 0.1),
            ..default()
        });
        Self { mesh, material }
    }
}

#[derive(Component)]
pub struct SparkParticle {
    velocity: Vec3,
    elapsed: f32,
}

// Tiny emissive cubes scattered around the post-bounce direction. Rate
// limiting lives with the caller (shared with the bounce sound).
pub fn spawn_bounce_sparks(commands: &mut Commands, spark_assets: &SparkAssets, pos: Vec3, bounce_dir: Vec3) {
    let mut rng = rng();
    for _ in 0..PROJECTILE_SPARK_COUNT {
        let scatter = Vec3::new(
            rng.random_range(-1.0..1.0),
            rng.random_range(-1.0..1.0),
            rng.random_range(-1.0..1.0),
        );
        let direction = (bounce_dir + scatter * 0.8).normalize_or_zero();
        commands.spawn((
            Mesh3d(spark_assets.mesh.clone()),
            MeshMaterial3d(spark_assets.material.clone()),
            NotShadowCaster,
            Transform::from_translation(pos),
            SparkParticle {
                velocity: direction * PROJECTILE_SPARK_SPEED * rng.random_range(0.5..1.3),
                elapsed: 0.0,
            },
        ));
    }
}

// Ballistic fade-out: sparks fly, drop, and shrink to nothing.
pub fn spark_particles_system(
    mut commands: Commands,
    time: Res<Time>,
    mut sparks: Query<(Entity, &mut SparkParticle, &mut Transform)>,
) {
    let delta = time.delta_secs();
    for (entity, mut spark, mut transform) in &mut sparks {
        spark.elapsed += delta;
        if spark.elapsed >= PROJECTILE_SPARK_LIFETIME_SECS {
            commands.entity(entity).despawn();
            continue;
        }
        spark.velocity.y -= PROJECTILE_SPARK_GRAVITY * delta;
        transform.translation += spark.velocity * delta;
        transform.scale = Vec3::splat(1.0 - spark.elapsed / PROJECTILE_SPARK_LIFETIME_SECS);
    }
}
