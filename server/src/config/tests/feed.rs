use crate::config::fixtures;

#[test]
fn feed_rejects_missing_actor_kind() {
    let mut config = fixtures::server_config();
    config.feed.actor_destroyed.remove("scuttler");
    let err = config
        .feed
        .validate(&config.actors.kinds)
        .expect_err("missing kind must fail");
    assert!(err.to_string().contains("feed.actor_destroyed"));
    assert!(err.to_string().contains("scuttler"));
}

#[test]
fn feed_rejects_unknown_actor_kind() {
    let mut config = fixtures::server_config();
    config.feed.actor_destroyed.insert("banana".to_owned(), true);
    let err = config
        .feed
        .validate(&config.actors.kinds)
        .expect_err("unknown kind must fail");
    assert!(err.to_string().contains("feed.actor_destroyed"));
    assert!(err.to_string().contains("banana"));
}
