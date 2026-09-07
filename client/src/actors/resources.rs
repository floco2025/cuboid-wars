use bevy::prelude::*;
use std::collections::HashMap;

use crate::network::accept_newer_tick;
use common::protocol::{ActorAnchor, ActorId, PlayerId};

// Actor information (client-side).
pub struct ActorInfo {
    pub entity: Entity,
    // Kind string from the wire `Actor.kind`. Used to look up per-kind
    // model, sounds, and effects when this actor is destroyed.
    pub kind: String,
    pub anchor: Option<ActorAnchor>,
    pub beam: ContinuousBeamState,
}

#[derive(Default)]
pub struct ContinuousBeamState {
    pub target: Option<PlayerId>,
    tick: Option<u32>,
}

impl ContinuousBeamState {
    pub fn apply(&mut self, tick: u32, target: Option<PlayerId>) {
        if accept_newer_tick(&mut self.tick, tick) {
            self.target = target;
        }
    }
}

// Map of all server-controlled actors.
#[derive(Resource, Default)]
pub struct ActorMap(HashMap<ActorId, ActorInfo>);

impl ActorMap {
    // "zapper#22" for logs; "actor#22" for unknown ids.
    #[must_use]
    pub fn describe(&self, id: &ActorId) -> String {
        self.get(id)
            .map_or_else(|| format!("actor#{}", id.0), |info| format!("{}#{}", info.kind, id.0))
    }

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

    pub fn get_mut(&mut self, id: &ActorId) -> Option<&mut ActorInfo> {
        self.0.get_mut(id)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn continuous_beam_recovers_from_snapshots_without_accepting_stale_cues() {
        let mut beam = ContinuousBeamState::default();
        beam.apply(10, Some(PlayerId(1)));
        beam.apply(12, None);
        beam.apply(11, Some(PlayerId(2)));
        assert_eq!(beam.target, None);
        beam.apply(13, Some(PlayerId(2)));
        beam.apply(12, None);
        assert_eq!(beam.target, Some(PlayerId(2)));
        beam.apply(14, None);
        assert_eq!(beam.target, None);
    }

    #[test]
    fn continuous_beam_orders_ticks_across_wraparound() {
        let mut beam = ContinuousBeamState::default();
        beam.apply(u32::MAX, Some(PlayerId(1)));
        beam.apply(0, Some(PlayerId(2)));
        beam.apply(u32::MAX, None);
        assert_eq!(beam.target, Some(PlayerId(2)));
    }
}
