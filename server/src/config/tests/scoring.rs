use super::*;

fn fixture() -> ScoringConfig {
    ScoringConfig {
        player_kill: 200,
        player_death: -200,
        gold: 1000,
        actor_hit: HashMap::from([("zapper".to_owned(), 5)]),
        actor_kill: HashMap::from([("zapper".to_owned(), 150)]),
    }
}

fn one_actor_kind(kind: &str) -> HashMap<String, ()> {
    HashMap::from([(kind.to_owned(), ())])
}

#[test]
fn accepts_matching_maps() {
    fixture()
        .validate(&one_actor_kind("zapper"))
        .expect("matching maps should pass");
}

#[test]
fn rejects_missing_actor_kind() {
    let mut scoring = fixture();
    scoring.actor_hit.clear();
    let err = scoring
        .validate(&one_actor_kind("zapper"))
        .expect_err("missing actor_hit kind must be rejected");
    assert!(err.to_string().contains("scoring.actor_hit"));
}

#[test]
fn rejects_unknown_actor_kind() {
    let mut scoring = fixture();
    scoring.actor_kill.insert("banana".to_owned(), 1);
    let err = scoring
        .validate(&one_actor_kind("zapper"))
        .expect_err("unknown actor_kill kind must be rejected");
    assert!(err.to_string().contains("scoring.actor_kill"));
}
