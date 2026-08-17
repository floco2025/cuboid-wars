use bevy::prelude::*;
use rand::{Rng, RngExt, rng};
use std::f32::consts::TAU;

use super::particles::{ParticleCloud, ParticleSpawn};
use crate::constants::*;

#[derive(Clone, Copy)]
pub enum ImpactKind {
    World,
    Character,
    Barrier(Color),
}

pub fn spawn_impact_sparks(
    sparks: &mut ParticleCloud,
    position: Vec3,
    surface_normal: Vec3,
    outgoing_direction: Vec3,
    impact_speed: f32,
    kind: ImpactKind,
) {
    let normal = surface_normal.normalize_or(Vec3::Y);
    let outgoing = outgoing_direction.normalize_or(normal);
    let cone_axis = (normal * 0.7 + outgoing * 0.3).normalize_or(normal);
    let count = impact_particle_count(impact_speed);
    let speed_scale = (impact_speed / PROJECTILE_SPARK_REFERENCE_SPEED).clamp(0.5, 1.5).sqrt();
    let base_color = impact_color(kind) * PROJECTILE_SPARK_EMISSIVE;
    let mut rng = rng();

    for _ in 0..count {
        let direction = outward_cone_direction(
            &mut rng,
            cone_axis,
            normal,
            PROJECTILE_SPARK_SPREAD_DEGREES.to_radians(),
        );
        let size = PROJECTILE_SPARK_SIZE * rng.random_range(0.65..1.35);
        sparks.spawn(ParticleSpawn {
            position: position + normal * 0.015,
            velocity: direction * PROJECTILE_SPARK_SPEED * speed_scale * rng.random_range(0.65..1.35),
            acceleration: Vec3::NEG_Y * PROJECTILE_SPARK_GRAVITY,
            start_size: size,
            end_size: 0.0,
            stretch: Vec3::ONE,
            fades: true,
            lifetime: PROJECTILE_SPARK_LIFETIME_SECS * rng.random_range(0.75..1.25),
            color: base_color * rng.random_range(0.75..1.2),
        });
    }
}

fn impact_particle_count(speed: f32) -> usize {
    let min = (PROJECTILE_SPARK_BASE_COUNT / 2).max(1);
    let max = PROJECTILE_SPARK_BASE_COUNT.saturating_mul(5).div_ceil(3);
    let scaled = PROJECTILE_SPARK_BASE_COUNT as f32 * speed.max(0.0) / PROJECTILE_SPARK_REFERENCE_SPEED;
    (scaled.round() as usize).clamp(min, max)
}

fn impact_color(kind: ImpactKind) -> Vec3 {
    let color = match kind {
        ImpactKind::World => Color::srgb(1.0, 0.78, 0.22),
        ImpactKind::Character => Color::srgb(1.0, 0.38, 0.08),
        ImpactKind::Barrier(color) => color,
    }
    .to_linear();
    Vec3::new(color.red, color.green, color.blue)
}

fn outward_cone_direction(rng: &mut impl Rng, axis: Vec3, surface_normal: Vec3, spread_radians: f32) -> Vec3 {
    let axis = axis.normalize_or(surface_normal);
    let tangent = axis.any_orthonormal_vector();
    let bitangent = axis.cross(tangent).normalize_or(Vec3::Y);
    let cos_min = spread_radians.cos();
    let cos_theta = rng.random_range(cos_min..=1.0);
    let sin_theta = (1.0 - cos_theta * cos_theta).sqrt();
    let azimuth = rng.random_range(0.0..TAU);
    let mut direction = axis * cos_theta + (tangent * azimuth.cos() + bitangent * azimuth.sin()) * sin_theta;
    let normal_component = direction.dot(surface_normal);
    if normal_component < 0.0 {
        direction -= 2.0 * normal_component * surface_normal;
    }
    direction.normalize_or(surface_normal)
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;

    use super::*;

    #[test]
    fn impact_particle_count_scales_and_clamps() {
        assert_eq!(impact_particle_count(0.0), 3);
        assert_eq!(
            impact_particle_count(PROJECTILE_SPARK_REFERENCE_SPEED),
            PROJECTILE_SPARK_BASE_COUNT
        );
        assert_eq!(impact_particle_count(PROJECTILE_SPARK_REFERENCE_SPEED * 10.0), 10);
    }

    #[test]
    fn cone_directions_stay_outside_the_surface() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(7);
        let normal = Vec3::Y;
        for _ in 0..1_000 {
            let direction = outward_cone_direction(&mut rng, Vec3::X, normal, 70.0_f32.to_radians());
            assert!(direction.dot(normal) >= 0.0);
        }
    }
}
