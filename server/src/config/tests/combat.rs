use crate::config::ServerGameplayConfig;
use crate::config::fixtures;

fn config() -> ServerGameplayConfig {
    fixtures::server_config()
}

#[test]
fn health_rejects_missing_actor_kind() {
    let mut config = config();
    config.combat.health.actors.remove("scuttler");
    let err = config
        .combat
        .validate(&config.actors.kinds)
        .expect_err("missing kind must fail");
    assert!(err.to_string().contains("combat.health.actors"));
    assert!(err.to_string().contains("scuttler"));
}

#[test]
fn damage_rejects_unknown_actor_kind() {
    let mut config = config();
    let zapper = *config.combat.damage.expect_actor("zapper");
    config.combat.damage.actors.insert("banana".to_owned(), zapper);
    let err = config
        .combat
        .validate(&config.actors.kinds)
        .expect_err("unknown kind must fail");
    assert!(err.to_string().contains("combat.damage.actors"));
    assert!(err.to_string().contains("banana"));
}

#[test]
fn damage_rejects_beam_dps_on_contact_kind() {
    let mut config = config();
    config
        .combat
        .damage
        .actors
        .get_mut("scuttler")
        .expect("scuttler damage config")
        .beam_dps = Some(1.0);
    let err = config
        .combat
        .validate(&config.actors.kinds)
        .expect_err("beam dps on a contact kind must fail");
    assert!(err.to_string().contains("fires no beam"));
}

#[test]
fn damage_requires_beam_dps_on_beam_kind() {
    let mut config = config();
    config
        .combat
        .damage
        .actors
        .get_mut("zapper")
        .expect("zapper damage config")
        .beam_dps = None;
    let err = config
        .combat
        .validate(&config.actors.kinds)
        .expect_err("missing beam dps on a beam kind must fail");
    assert!(err.to_string().contains("fires a beam"));
}

#[test]
fn potion_heal_must_be_in_unit_interval() {
    for fraction in [0.0, 1.5] {
        let mut config = config();
        config.combat.health.player.potion_heal = fraction;
        let err = config
            .combat
            .validate(&config.actors.kinds)
            .expect_err("out-of-range potion fraction must fail");
        assert!(err.to_string().contains("potion_heal"));
    }
}
