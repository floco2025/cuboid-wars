use bincode::{Decode, Encode};
use serde::Deserialize;

use super::ids::QuestId;

// Whose progress a quest tracks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuestScope {
    // Own progress, own completion.
    Individual,
    // One pooled counter for all players; any player's event advances it,
    // and it completes once for the group.
    Shared,
    // Own progress per player; the group completes once every logged-in
    // player reached the threshold.
    Everyone,
}

impl QuestScope {
    // `Shared` and `Everyone` complete for the group at once (and can gate
    // other quests); `Individual` completes per player.
    #[must_use]
    pub fn is_group(self) -> bool {
        !matches!(self, Self::Individual)
    }
}

// Group-visible state of an unlocked group-scoped quest.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct QuestGroupStatus {
    pub id: QuestId,
    pub completed: bool,
    pub progress: QuestGroupProgress,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum QuestGroupProgress {
    Shared { progress: u32 },
    // Players at the threshold, and players logged in.
    Everyone { players_done: u32, players_total: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct QuestInitialStatus {
    pub completed: bool,
    pub progress: QuestInitialProgress,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum QuestInitialProgress {
    Individual {
        progress: u32,
    },
    Shared {
        progress: u32,
    },
    Everyone {
        progress: u32,
        players_done: u32,
        players_total: u32,
    },
}
