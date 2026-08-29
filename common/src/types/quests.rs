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

// Group-visible state of an unlocked `Shared` / `Everyone` quest.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct QuestGroupStatus {
    pub id: QuestId,
    pub completed: bool,
    // `Shared` only.
    pub shared_progress: u32,
    // `Everyone` only: players at the threshold, and players logged in.
    pub players_done: u32,
    pub players: u32,
}
