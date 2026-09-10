use std::collections::HashMap;

use anyhow::Result;
use serde::Deserialize;

use super::validation::validate_covers_actor_kinds;

// Which feed lines everyone sees. The one broadcast gate: `emit_feed`
// consults it for public audiences, nothing else decides.
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
    pub switch_on: bool,
    pub switch_off: bool,
    pub admin_action: bool,
    pub chat: bool,
}

impl FeedConfig {
    pub(super) fn validate<T>(&self, actors: &HashMap<String, T>) -> Result<()> {
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
            switch_on: enabled,
            switch_off: enabled,
            admin_action: enabled,
            chat: enabled,
        }
    }
}

#[cfg(test)]
#[path = "tests/feed.rs"]
mod tests;
