use super::*;
use crate::protocol::CarrierId;

#[test]
fn airborne_momentum_does_not_decay_between_airborne_steps() {
    let momentum = AirborneMomentum(Vec3::new(3.0, 0.0, -6.0));

    assert_eq!(momentum.step(0.1), Vec3::new(0.3, 0.0, -0.6));
    assert_eq!(momentum.step(0.1), Vec3::new(0.3, 0.0, -0.6));
}

#[test]
fn airborne_momentum_ends_on_support_or_collision() {
    let airborne = CharacterMovementResult {
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
    let mut momentum = AirborneMomentum(Vec3::X);
    momentum.finish_step(&airborne);
    assert_eq!(momentum.0, Vec3::X);

    let mut landed = airborne;
    landed.support = CharacterSupport::Ground;
    momentum.finish_step(&landed);
    assert_eq!(momentum.0, Vec3::ZERO);

    let mut blocked = airborne;
    blocked.blocked = true;
    let mut momentum = AirborneMomentum(Vec3::X);
    momentum.finish_step(&blocked);
    assert_eq!(momentum.0, Vec3::ZERO);
}
