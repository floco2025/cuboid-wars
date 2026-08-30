use std::collections::HashMap;

use anyhow::Result;
use serde::Deserialize;

use super::{actors::ActorKindServerConfig, validation::validate_covers_actor_kinds};

// Which feed lines everyone sees. The one broadcast gate: `announce`
// consults it, nothing else decides.
#[derive(Debug, Clone, Deserialize)]
pub struct FeedConfig {
    pub player_joined: bool,
    pub player_left: bool,
    pub player_died: bool,
    // Per actor kind; must cover exactly the configured kinds.
    pub actor_destroyed: HashMap<String, bool>,
    pub key_found: bool,
    pub quest_completed: bool,
    pub quest_part_done: bool,
    pub group_quest_completed: bool,
    pub barrier_opened: bool,
    pub barrier_closed: bool,
    pub admin_action: bool,
    pub chat: bool,
}

impl FeedConfig {
    pub(super) fn validate(&self, actors: &HashMap<String, ActorKindServerConfig>) -> Result<()> {
        validate_covers_actor_kinds(self.actor_destroyed.keys(), actors, "feed.actor_destroyed")
    }

    // Every switch set to `enabled`, with the given actor kinds.
    #[cfg(test)]
    pub(crate) fn all(enabled: bool, actor_kinds: &[&str]) -> Self {
        Self {
            player_joined: enabled,
            player_left: enabled,
            player_died: enabled,
            actor_destroyed: actor_kinds.iter().map(|kind| ((*kind).to_owned(), enabled)).collect(),
            key_found: enabled,
            quest_completed: enabled,
            quest_part_done: enabled,
            group_quest_completed: enabled,
            barrier_opened: enabled,
            barrier_closed: enabled,
            admin_action: enabled,
            chat: enabled,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::config::ServerGameplayConfig;

    #[test]
    fn feed_rejects_missing_actor_kind() {
        let mut config = ServerGameplayConfig::load_default().expect("default server gameplay config should load");
        config.feed.actor_destroyed.remove("mine");
        let err = config
            .feed
            .validate(&config.actors)
            .expect_err("missing kind must fail");
        assert!(err.to_string().contains("feed.actor_destroyed"));
        assert!(err.to_string().contains("mine"));
    }

    #[test]
    fn feed_rejects_unknown_actor_kind() {
        let mut config = ServerGameplayConfig::load_default().expect("default server gameplay config should load");
        config.feed.actor_destroyed.insert("banana".to_owned(), true);
        let err = config
            .feed
            .validate(&config.actors)
            .expect_err("unknown kind must fail");
        assert!(err.to_string().contains("feed.actor_destroyed"));
        assert!(err.to_string().contains("banana"));
    }
}
