use super::*;
use crate::physics::{CharacterMovementResult, CharacterSupport};
use crate::protocol::CarrierId;

#[test]
fn horizontal_velocity_does_not_decay_between_airborne_steps() {
    let momentum = HorizontalVelocity(Vec3::new(3.0, 0.0, -6.0));

    assert_eq!(momentum.step(0.1), Vec3::new(0.3, 0.0, -0.6));
    assert_eq!(momentum.step(0.1), Vec3::new(0.3, 0.0, -0.6));
}

#[test]
fn landing_preserves_horizontal_velocity_and_wall_contact_clips_only_into_wall() {
    let airborne = CharacterMovementResult {
        contact_normals: [Vec3::ZERO; 5],
        impact_speed: 0.0,
        grounding: Default::default(),
        position: Default::default(),
        vertical_velocity: 1.0,
        support: CharacterSupport::Airborne,
        blocked: false,
        carrier: CarrierId::WORLD,
        floor_velocity: Vec3::ZERO,
        lifted: false,
        crushed: false,
    };
    let mut momentum = HorizontalVelocity(Vec3::X);
    momentum.finish_step(&airborne);
    assert_eq!(momentum.0, Vec3::X);

    let mut landed = airborne;
    landed.support = CharacterSupport::Ground;
    momentum.finish_step(&landed);
    assert_eq!(momentum.0, Vec3::X);

    let mut blocked = airborne;
    blocked.blocked = true;
    blocked.contact_normals[0] = -Vec3::X;
    let mut momentum = HorizontalVelocity(Vec3::X);
    momentum.finish_step(&blocked);
    assert_eq!(momentum.0, Vec3::ZERO);
}
