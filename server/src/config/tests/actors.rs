use super::*;
use serde_json::json;

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
fn actor_kind_requires_explicit_respawn_setting() {
    let mut value = json!({
        "movement_collider": { "diameter": 0.6, "height": 1.8 },
        "hitbox": { "width": 1.0, "height": 1.3, "depth": 0.6, "bottom_offset": 0.5 },
        "eye_height": 1.0,
        "can_use_ladders": false,
        "immovable": false,
        "vision_range": 10.0,
        "roam_steps": 2,
        "attack": { "type": "contact", "trigger_gap": 0.1 }
    });

    let err =
        serde_json::from_value::<ActorKindServerConfig>(value.clone()).expect_err("respawn_secs must be explicit");

    assert!(err.to_string().contains("respawn_secs"));

    value["respawn_secs"] = serde_json::Value::Null;
    let actor =
        serde_json::from_value::<ActorKindServerConfig>(value).expect("null should explicitly disable respawning");
    assert_eq!(actor.respawn_secs, None);
}
