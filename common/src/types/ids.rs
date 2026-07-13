use bevy_ecs::prelude::*;
use bincode::{Decode, Encode};
use serde::Deserialize;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Component, Encode, Decode)]
pub struct PlayerId(pub u32);

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Component, Encode, Decode)]
pub struct ActorId(pub u32);

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Component, Encode, Decode)]
pub struct ItemId(pub u32);

// Stable per-quest identifier carried on `SQuestsAssigned` / `SQuestProgress`
// / `SQuestCompleted` and keyed in per-player progress maps. Strings match the
// human-readable `id` in `gameplay.json`'s `quests` list.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Encode, Decode, Deserialize)]
pub struct QuestId(pub String);
