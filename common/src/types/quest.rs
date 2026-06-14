use bincode::{Decode, Encode};
use serde::Deserialize;

// Stable per-quest identifier carried on `SQuestsAssigned` / `SQuestProgress`
// / `SQuestCompleted` and keyed in per-player progress maps. Strings match the
// human-readable `id` in `gameplay.json`'s `quests` list.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Encode, Decode, Deserialize)]
pub struct QuestId(pub String);
