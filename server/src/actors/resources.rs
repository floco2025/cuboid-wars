use std::collections::{HashMap, VecDeque};

use bevy::prelude::*;

use common::{
    physics::CharacterSupport,
    protocol::{ActorId, PlayerId, Position},
};

use super::navigation::{NavNode, PlannedRoute};
use crate::watchdog::ProgressWatchdog;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) enum ActorMode {
    #[default]
    Roam,
    Engage {
        target: PlayerId,
        target_pos: Position,
    },
    Evade,
    ReturnHome,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum BeamState {
    Ready,
    Firing { target: PlayerId, remaining_secs: f32 },
    Cooldown { remaining_secs: f32 },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ActorRoute {
    pub waypoints: VecDeque<Position>,
    pub destination: Position,
    pub destination_node: NavNode,
}

impl ActorRoute {
    #[must_use]
    pub(crate) fn new(planned: PlannedRoute) -> Option<Self> {
        let destination = *planned.waypoints.back()?;
        Some(Self {
            waypoints: planned.waypoints,
            destination,
            destination_node: planned.destination_node,
        })
    }

    #[must_use]
    pub fn next(&self) -> Option<Position> {
        self.waypoints.front().copied()
    }

    pub(crate) fn retarget(&mut self, target: Position) {
        if let Some(last) = self.waypoints.back_mut() {
            *last = target;
            self.destination = target;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct AwarePlayer {
    pub(crate) id: PlayerId,
    pub(crate) pos: Position,
    pub(crate) support: CharacterSupport,
    pub(crate) visible: bool,
    pub(crate) forget_remaining_secs: f32,
    pub(crate) attack_anchor: Option<Position>,
}

pub struct ActorInfo {
    pub entity: Entity,
    pub spawn_zone_index: usize,
    pub spawn_kind: String,
    pub(crate) mode: ActorMode,
    pub(crate) route: Option<ActorRoute>,
    pub(crate) beam: BeamState,
    pub(crate) awareness: Vec<AwarePlayer>,
    pub(crate) decision_timer: f32,
    pub(crate) watchdog: ProgressWatchdog,
    pub(crate) evade_replan_remaining_secs: f32,
    // Player who landed the last projectile damage. Read by
    // `actors_removal_system` when the actor's health hits zero, so the
    // `SActorDeath` broadcast can attribute the kill. Chain-explosion
    // damage doesn't touch this field — those deaths read `None`.
    pub last_damager: Option<PlayerId>,
}

impl ActorInfo {
    #[must_use]
    pub fn new(entity: Entity, spawn_zone_index: usize, spawn_kind: String) -> Self {
        Self {
            entity,
            spawn_zone_index,
            spawn_kind,
            mode: ActorMode::Roam,
            route: None,
            beam: BeamState::Ready,
            awareness: Vec::new(),
            decision_timer: 0.0,
            watchdog: ProgressWatchdog::default(),
            evade_replan_remaining_secs: 0.0,
            last_damager: None,
        }
    }

    pub(crate) fn set_route(&mut self, route: Option<ActorRoute>) {
        self.route = route;
        self.watchdog.reset();
    }
}

#[derive(Resource, Default)]
pub struct ActorMap(HashMap<ActorId, ActorInfo>);

impl ActorMap {
    pub fn insert(&mut self, id: ActorId, info: ActorInfo) -> Option<ActorInfo> {
        self.0.insert(id, info)
    }

    pub fn remove(&mut self, id: &ActorId) -> Option<ActorInfo> {
        self.0.remove(id)
    }

    // "zapper#22" for logs; "actor#22" for unknown ids.
    #[must_use]
    pub fn describe(&self, id: &ActorId) -> String {
        self.get(id).map_or_else(
            || format!("actor#{}", id.0),
            |info| format!("{}#{}", info.spawn_kind, id.0),
        )
    }

    #[must_use]
    pub fn get(&self, id: &ActorId) -> Option<&ActorInfo> {
        self.0.get(id)
    }

    pub fn get_mut(&mut self, id: &ActorId) -> Option<&mut ActorInfo> {
        self.0.get_mut(id)
    }

    pub fn values(&self) -> impl Iterator<Item = &ActorInfo> {
        self.0.values()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&ActorId, &ActorInfo)> {
        self.0.iter()
    }
}

#[derive(Resource, Default)]
pub struct ActorSpawner {
    pub next_id: u32,
}

#[derive(Resource, Default)]
pub struct ActorSpawnThrottles(pub HashMap<usize, f32>);

// A spawn that has been decided (id, spot, and heading reserved) but whose
// beam-in warning window hasn't elapsed. The actor entity doesn't exist yet —
// clients render a ghost from the snapshot's `spawning_actors` list. Counts
// toward the zone quota and occupies its spot, since materialization is
// unconditional.
pub struct PendingActorSpawn {
    pub actor_id: ActorId,
    pub zone_idx: usize,
    pub kind: String,
    pub pos: Position,
    pub face_yaw: f32,
    pub remaining_secs: f32,
    // The full window length, shipped alongside `remaining_secs` so clients
    // can compute and animate the fade fraction.
    pub warning_secs: f32,
}

#[derive(Resource, Default)]
pub struct PendingActorSpawns(pub Vec<PendingActorSpawn>);

impl ActorSpawner {
    pub fn allocate(&mut self) -> ActorId {
        let id = ActorId(self.next_id);
        self.next_id = self.next_id.wrapping_add(1);
        id
    }
}
