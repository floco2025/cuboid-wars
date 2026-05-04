use bevy::prelude::*;
use std::collections::HashMap;

use common::protocol::ActorId;

// Actor information (client-side).
pub struct ActorInfo {
    pub entity: Entity,
    // Kind string from the wire `Actor.kind`. Used to look up per-kind
    // model, sounds, and effects when this actor is destroyed.
    pub kind: String,
}

// Map of all server-controlled actors.
#[derive(Resource, Default)]
pub struct ActorMap(pub HashMap<ActorId, ActorInfo>);
