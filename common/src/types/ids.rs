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

// Which rigid group of map records a thing belongs to. `WORLD` is the map
// itself, which never moves; `CarrierId(n)` for n >= 1 names
// `MapLayout.carriers[n - 1]`.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Encode, Decode)]
pub struct CarrierId(pub u16);

impl CarrierId {
    pub const WORLD: Self = Self(0);

    #[must_use]
    pub const fn is_world(self) -> bool {
        self.0 == 0
    }

    // Index into `MapLayout.carriers`; `None` for the world.
    #[must_use]
    pub const fn carried_index(self) -> Option<usize> {
        match self.0 {
            0 => None,
            n => Some(n as usize - 1),
        }
    }
}

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
