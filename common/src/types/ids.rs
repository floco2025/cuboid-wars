use bevy_ecs::prelude::*;
use bincode::{Decode, Encode};
use serde::Deserialize;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Component, Encode, Decode)]
pub struct PlayerId(pub u32);

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Component, Encode, Decode)]
pub struct ActorId(pub u32);

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Component, Encode, Decode)]
pub struct ItemId(pub u32);

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Component, Encode, Decode)]
pub struct MissileId(pub u32);

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Encode, Decode)]
pub struct PortalPairId(pub u32);

// What a missile homes on. Carried in `CMissileShot`; guidance is server-only,
// so it never rides snapshots or intents.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HomingTarget {
    Player(PlayerId),
    Actor(ActorId),
}

// Stable per-quest identifier carried by `SQuestUpdates` and snapshots, and
// keyed in per-player progress maps. Strings match the
// human-readable `id` in one map's `gameplay.json` quest list.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Encode, Decode, Deserialize)]
pub struct QuestId(pub String);
