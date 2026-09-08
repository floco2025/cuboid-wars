use bevy::prelude::*;
use std::collections::HashMap;

use crate::network::accept_newer_tick;
use common::{
    constants::TICK_SECS,
    protocol::{ActorAnchor, ActorBeam, ActorId},
};

// Actor information (client-side).
pub struct ActorInfo {
    pub entity: Entity,
    // Kind string from the wire `Actor.kind`. Used to look up per-kind
    // model, sounds, and effects when this actor is destroyed.
    pub kind: String,
    pub anchor: Option<ActorAnchor>,
    pub beam: ActorBeamState,
}

#[derive(Default)]
pub struct ActorBeamState {
    beam: Option<ActorBeam>,
    tick: Option<u32>,
}

impl ActorBeamState {
    pub fn active(&self, tick: u32) -> Option<ActorBeam> {
        let mut beam = self.beam?;
        let elapsed_ticks = (tick.wrapping_sub(self.tick?) as i32).max(0);
        beam.remaining_secs -= elapsed_ticks as f32 * TICK_SECS;
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
mod tests {
    use super::*;
    use common::protocol::PlayerId;

    fn burst(target: u32) -> Option<ActorBeam> {
        Some(ActorBeam {
            target: PlayerId(target),
            started_tick: 10,
            remaining_secs: 2.0,
        })
    }

    #[test]
    fn beam_recovers_from_snapshots_without_accepting_stale_cues() {
        let mut beam = ActorBeamState::default();
        beam.apply(10, burst(1));
        beam.apply(12, None);
        beam.apply(11, burst(2));
        assert_eq!(beam.active(12), None);
        beam.apply(13, burst(2));
        beam.apply(12, None);
        assert_eq!(beam.active(13), burst(2));
        beam.apply(14, None);
        assert_eq!(beam.active(14), None);
    }

    #[test]
    fn beam_orders_and_expires_across_tick_wraparound() {
        let mut beam = ActorBeamState::default();
        beam.apply(u32::MAX, burst(1));
        beam.apply(0, burst(2));
        beam.apply(u32::MAX, None);
        assert_eq!(beam.active(0), burst(2));
        let active = beam.active(30).expect("beam expired early");
        assert!((active.remaining_secs - 1.0).abs() < 0.0001);
        assert_eq!(beam.active(61), None);
    }

    #[test]
    fn late_snapshot_uses_remaining_time_without_extending_the_burst() {
        let mut beam = ActorBeamState::default();
        beam.apply(
            40,
            Some(ActorBeam {
                target: PlayerId(1),
                started_tick: 10,
                remaining_secs: 1.0,
            }),
        );
        assert!((beam.active(55).expect("beam expired early").remaining_secs - 0.5).abs() < 0.0001);
        assert_eq!(beam.active(71), None);
        beam.apply(
            60,
            Some(ActorBeam {
                target: PlayerId(1),
                started_tick: 10,
                remaining_secs: 10.0 * TICK_SECS,
            }),
        );
        assert_eq!(beam.active(71), None);
    }
}
