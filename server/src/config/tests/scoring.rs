use super::*;

#[test]
fn actor_kind_maps_must_cover_exactly_the_configured_kinds() {
    let scoring = ScoringConfig {
        player_kill: 200,
        player_death: -200,
        gold: 1000,
        actor_hit: HashMap::from([("zapper".to_owned(), 5)]),
        actor_kill: HashMap::from([("zapper".to_owned(), 150)]),
    };
    let kinds = HashMap::from([("zapper".to_owned(), ())]);
    scoring.validate(&kinds, "scoring").expect("matching maps rejected");

    let mut missing = scoring.clone();
    missing.actor_hit.clear();
    let err = missing
        .validate(&kinds, "scoring")
        .expect_err("missing actor_hit kind accepted");
    assert!(err.to_string().contains("scoring.actor_hit"), "{err}");

    let mut unknown = scoring;
    unknown.actor_kill.insert("banana".to_owned(), 1);
    let err = unknown
        .validate(&kinds, "scoring")
        .expect_err("unknown actor_kill kind accepted");
    assert!(err.to_string().contains("scoring.actor_kill"), "{err}");
}
