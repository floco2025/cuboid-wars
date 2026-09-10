use super::*;

#[test]
fn momentum_displacement_combines_blast_and_airborne_velocity() {
    let knockback = KnockbackVelocity(Vec3::X * 2.0);
    let momentum = AirborneMomentum(Vec3::Z * 3.0);

    assert_eq!(
        momentum_displacement(Some(&knockback), Some(&momentum), 0.5),
        Vec3::new(1.0, 0.0, 1.5)
    );
}
