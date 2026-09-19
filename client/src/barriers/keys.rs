use bevy::prelude::*;
use common::protocol::FieldId;

// Fields the map places a key for, from `SInit`; the HUD shows one
// key slot per entry. Starts empty (initialized at app build).
#[derive(Resource, Default)]
pub struct KeyFields(pub Vec<FieldId>);
