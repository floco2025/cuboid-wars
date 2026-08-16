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

    pub fn iter(&self) -> impl Iterator<Item = (&ActorId, &ActorInfo)> {
        self.0.iter()
    }

    pub fn retain(&mut self, f: impl FnMut(&ActorId, &mut ActorInfo) -> bool) {
        self.0.retain(f);
    }
}

// Beam-in ghosts, keyed by the reserved id from the snapshot's
// `spawning_actors`. Purely visual — the real actor arrives under the same
// id via the `ActorMap` diff when the warning window ends.
#[derive(Resource, Default)]
pub struct ActorGhostMap(HashMap<ActorId, Entity>);

impl ActorGhostMap {
    pub fn insert(&mut self, id: ActorId, entity: Entity) -> Option<Entity> {
        self.0.insert(id, entity)
    }

    #[must_use]
    pub fn get(&self, id: &ActorId) -> Option<Entity> {
        self.0.get(id).copied()
    }

    pub fn retain(&mut self, f: impl FnMut(&ActorId, &mut Entity) -> bool) {
        self.0.retain(f);
    }
}
