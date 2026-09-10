use bevy::prelude::*;
use std::collections::HashMap;

use crate::network::accept_newer_tick;
use common::{
    config::NetworkConfig,
    protocol::{ActorBeam, ActorId},
};

// Actor information (client-side).
pub struct ActorInfo {
    pub entity: Entity,
    // Kind string from the wire `Actor.kind`. Used to look up per-kind
    // model, sounds, and effects when this actor is destroyed.
    pub kind: String,
    pub beam: ActorBeamState,
}

#[derive(Default)]
pub struct ActorBeamState {
    beam: Option<ActorBeam>,
    tick: Option<u32>,
}

impl ActorBeamState {
    pub fn active(&self, tick: u32, network: &NetworkConfig) -> Option<ActorBeam> {
        let mut beam = self.beam?;
        let elapsed_ticks = (tick.wrapping_sub(self.tick?) as i32).max(0);
        beam.remaining_secs -= elapsed_ticks as f32 * network.tick_secs();
        (beam.remaining_secs > 0.0).then_some(beam)
    }

    pub fn apply(&mut self, tick: u32, beam: Option<ActorBeam>) {
        if accept_newer_tick(&mut self.tick, tick) {
            self.beam = beam;
        }
    }
}

// Map of all server-controlled actors.
#[derive(Resource, Default)]
pub struct ActorMap {
    pub peaceful: bool,
    entries: HashMap<ActorId, ActorInfo>,
}

impl ActorMap {
    // "zapper#22" for logs; "actor#22" for unknown ids.
    #[must_use]
    pub fn describe(&self, id: &ActorId) -> String {
        self.get(id)
            .map_or_else(|| format!("actor#{}", id.0), |info| format!("{}#{}", info.kind, id.0))
    }

    pub fn insert(&mut self, id: ActorId, info: ActorInfo) -> Option<ActorInfo> {
        self.entries.insert(id, info)
    }

    pub fn remove(&mut self, id: &ActorId) -> Option<ActorInfo> {
        self.entries.remove(id)
    }

    #[must_use]
    pub fn contains_key(&self, id: &ActorId) -> bool {
        self.entries.contains_key(id)
    }

    #[must_use]
    pub fn get(&self, id: &ActorId) -> Option<&ActorInfo> {
        self.entries.get(id)
    }

    pub fn get_mut(&mut self, id: &ActorId) -> Option<&mut ActorInfo> {
        self.entries.get_mut(id)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&ActorId, &ActorInfo)> {
        self.entries.iter()
    }

    pub fn retain(&mut self, f: impl FnMut(&ActorId, &mut ActorInfo) -> bool) {
        self.entries.retain(f);
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

#[cfg(test)]
#[path = "tests/resources.rs"]
mod tests;
