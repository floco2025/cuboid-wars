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
    // Own progress per player; the group completes once every active
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
    // Active players at the threshold, and active players in total.
    Everyone { players_done: u32, players_total: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct QuestStatus {
    pub completed: bool,
    pub progress: QuestStateProgress,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum QuestStateProgress {
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
