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
