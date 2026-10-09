use crate::config::fixtures;

#[test]
fn beam_dps_is_set_exactly_when_the_kinds_attack_fires_a_beam() {
    let mut config = fixtures::server_config();
    config
        .combat
        .damage
        .actors
        .get_mut("scuttler")
        .expect("scuttler damage config missing")
        .beam_dps = Some(1.0);
    let err = config
        .combat
        .validate(&config.actors, "combat")
        .expect_err("beam dps on a contact kind accepted");
    assert!(err.to_string().contains("fires no beam"));

    let mut config = fixtures::server_config();
    config
        .combat
        .damage
        .actors
        .get_mut("zapper")
        .expect("zapper damage config missing")
        .beam_dps = None;
    let err = config
        .combat
        .validate(&config.actors, "combat")
        .expect_err("missing beam dps on a beam kind accepted");
    assert!(err.to_string().contains("fires a beam"));
}
