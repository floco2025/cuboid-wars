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
pub struct ActorMap(HashMap<ActorId, ActorInfo>);

impl ActorMap {
    pub fn insert(&mut self, id: ActorId, info: ActorInfo) -> Option<ActorInfo> {
        self.0.insert(id, info)
    }

    pub fn remove(&mut self, id: &ActorId) -> Option<ActorInfo> {
        self.0.remove(id)
    }

    #[must_use]
    pub fn contains_key(&self, id: &ActorId) -> bool {
        self.0.contains_key(id)
    }

    #[must_use]
    pub fn get(&self, id: &ActorId) -> Option<&ActorInfo> {
        self.0.get(id)
    }

    pub fn retain(&mut self, f: impl FnMut(&ActorId, &mut ActorInfo) -> bool) {
        self.0.retain(f);
    }
}
