use super::*;

#[test]
fn beam_attack_rejects_non_positive_duration() {
    let attack = ActorAttackConfig::Beam(ActorBeamAttackConfig {
        range: 15.0,
        duration_secs: 0.0,
        cooldown_secs: 5.0,
    });
    attack
        .validate("actors.test.attack")
        .expect_err("zero duration must fail");
}

#[test]
fn flying_actor_rejects_ground_only_abilities() {
    use common::config::ActorLocomotion;
    let mut actor = crate::actors::test_kinds::kind(crate::actors::test_kinds::BEAM);
    actor.character.locomotion = ActorLocomotion::Flying;
    actor.validate("flyer").expect("valid flying configuration rejected");
    actor.character.can_use_ladders = true;
    assert!(actor.validate("flyer").is_err());
    actor.character.can_use_ladders = false;
    actor.character.immovable = true;
    assert!(actor.validate("flyer").is_err());
}
