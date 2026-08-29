use std::collections::HashMap;

use anyhow::Result;
use serde::Deserialize;

use super::{actors::ActorKindServerConfig, validation::validate_covers_actor_kinds};
use common::protocol::FeedEvent;

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
    pub individual_quest_completed: bool,
    pub everyone_quest_part_done: bool,
    pub group_quest_completed: bool,
    pub barrier_opened: bool,
    pub barrier_closed: bool,
    pub admin_action: bool,
    pub chat: bool,
}

impl FeedConfig {
    #[must_use]
    pub fn announces(&self, event: &FeedEvent) -> bool {
        match event {
            FeedEvent::PlayerJoined { .. } => self.player_joined,
            FeedEvent::PlayerLeft { .. } => self.player_left,
            FeedEvent::PlayerDied { .. } => self.player_died,
            FeedEvent::ActorDestroyed { kind, .. } => self
                .actor_destroyed
                .get(kind)
                .copied()
                .expect("actor kind missing from feed.actor_destroyed"),
            FeedEvent::KeyFound { .. } => self.key_found,
            FeedEvent::QuestCompleted { .. } => self.individual_quest_completed,
            FeedEvent::EveryoneQuestPartDone { .. } => self.everyone_quest_part_done,
            FeedEvent::GroupQuestCompleted { .. } => self.group_quest_completed,
            FeedEvent::BarrierOpened { .. } => self.barrier_opened,
            FeedEvent::BarrierClosed { .. } => self.barrier_closed,
            // Replies go through `reply`, never the broadcast.
            FeedEvent::AdminReply { .. } => false,
            FeedEvent::AdminAction { .. } => self.admin_action,
            FeedEvent::Chat { .. } => self.chat,
        }
    }

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
            individual_quest_completed: enabled,
            everyone_quest_part_done: enabled,
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
    use super::*;
    use crate::config::ServerGameplayConfig;
    use common::protocol::DeathCause;

    #[test]
    fn announces_follows_the_switches() {
        let mut feed = FeedConfig::all(false, &["mine", "sentry"]);
        feed.actor_destroyed.insert("sentry".to_owned(), true);
        feed.player_died = true;
        let name = || "Marc".to_owned();

        assert!(feed.announces(&FeedEvent::PlayerDied {
            name: name(),
            cause: DeathCause::Fall,
        }));
        assert!(!feed.announces(&FeedEvent::PlayerJoined { name: name() }));
        assert!(feed.announces(&FeedEvent::ActorDestroyed {
            name: name(),
            kind: "sentry".to_owned(),
        }));
        assert!(!feed.announces(&FeedEvent::ActorDestroyed {
            name: name(),
            kind: "mine".to_owned(),
        }));
        assert!(!feed.announces(&FeedEvent::AdminReply {
            text: "/help".to_owned()
        }));
        assert!(!FeedConfig::all(true, &[]).announces(&FeedEvent::AdminReply {
            text: "/help".to_owned()
        }));
    }

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
