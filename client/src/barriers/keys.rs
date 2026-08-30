use bevy::prelude::*;
use common::protocol::BarrierKindId;

// Barrier kinds the map places a key for, from `SInit`; the HUD shows one
// key slot per entry. Starts empty (initialized at app build).
#[derive(Resource, Default)]
pub struct KeyKinds(pub Vec<BarrierKindId>);
