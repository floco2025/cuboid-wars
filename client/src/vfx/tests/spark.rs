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
