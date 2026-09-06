use std::collections::{HashMap, HashSet, VecDeque};

use bevy::prelude::*;

use common::{
    map::Carriers,
    physics::CharacterSupport,
    protocol::{ActorId, ActorMarker, ActorMoveIntent, CarrierId, FaceYaw, Health, PlayerId, Position},
};

use super::navigation::{NavNode, PlannedRoute};
use crate::watchdog::ProgressWatchdog;

// Whether this tick's movement left the actor inside a carrier's geometry;
// written by `apply_actor_moves`, read by `actors_removal_system`.
#[derive(Component, Default)]
pub struct ActorCrushed(pub bool);

pub type ActorStateQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static Position,
        &'static ActorMoveIntent,
        &'static FaceYaw,
        &'static Health,
    ),
    With<ActorMarker>,
>;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) enum ActorMode {
    #[default]
    Roam,
    Engage {
        target: PlayerId,
        target_pos: Position,
    },
    // `fleeing`: the route is a flight leg to a random cell, not cover.
    Evade {
        fleeing: bool,
    },
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

// An actor belongs to the carrier its zone is on for life: it navigates
// that carrier's grid in the carrier's frame (`route` is carrier-local) and
// despawns if it leaves the carrier's map. `mode` and `awareness` keep
// world positions, since perception and facing are world-space.
pub struct ActorInfo {
    pub entity: Entity,
    pub spawn_zone_index: usize,
    pub spawn_kind: String,
    pub carrier: CarrierId,
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
    pub fn new(entity: Entity, spawn_zone_index: usize, spawn_kind: String, carrier: CarrierId) -> Self {
        Self {
            entity,
            spawn_zone_index,
            spawn_kind,
            carrier,
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
pub struct ActorMap {
    entries: HashMap<ActorId, ActorInfo>,
    // Actor removal records the zone so its respawn timer starts at the lifecycle boundary.
    vacated_spawn_zones: HashSet<usize>,
}

impl ActorMap {
    pub fn insert(&mut self, id: ActorId, info: ActorInfo) -> Option<ActorInfo> {
        self.entries.insert(id, info)
    }

    pub fn remove(&mut self, id: &ActorId) -> Option<ActorInfo> {
        let info = self.entries.remove(id)?;
        self.vacated_spawn_zones.insert(info.spawn_zone_index);
        Some(info)
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
        self.entries.get(id)
    }

    pub fn get_mut(&mut self, id: &ActorId) -> Option<&mut ActorInfo> {
        self.entries.get_mut(id)
    }

    pub fn values(&self) -> impl Iterator<Item = &ActorInfo> {
        self.entries.values()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&ActorId, &ActorInfo)> {
        self.entries.iter()
    }

    pub(crate) fn drain_vacated_spawn_zones(&mut self) -> impl Iterator<Item = usize> + '_ {
        self.vacated_spawn_zones.drain()
    }

    #[must_use]
    pub fn has_vacated_spawn_zones(&self) -> bool {
        !self.vacated_spawn_zones.is_empty()
    }
}

#[derive(Resource, Default)]
pub struct ActorSpawner {
    pub next_id: u32,
}

#[derive(Resource, Default)]
pub struct ActorRespawnTimers(pub HashMap<usize, f32>);

// A spawn that has been decided (id, spot, and heading reserved) but whose
// beam-in warning window hasn't elapsed. The actor entity doesn't exist yet —
// clients render a ghost from the snapshot's `spawning_actors` list. Counts
// toward the zone quota and occupies its spot, since materialization is
// unconditional. `pos` is in the zone's carrier frame, so the spot rides
// the carrier through the window, which runs from `reserved_tick` to
// `due_tick` on the shared tick.
pub struct PendingActorSpawn {
    pub actor_id: ActorId,
    pub zone_idx: usize,
    pub kind: String,
    pub carrier: CarrierId,
    pub pos: Position,
    pub face_yaw: f32,
    pub reserved_tick: u32,
    pub due_tick: u32,
}

impl PendingActorSpawn {
    #[must_use]
    pub fn world_position(&self, carriers: &Carriers) -> Position {
        carriers.pose(self.carrier).transform_position(&self.pos)
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actor_map_records_vacated_zones() {
        let mut actors = ActorMap::default();
        let id = ActorId(4);
        let entity = Entity::from_bits(12);
        actors.insert(id, ActorInfo::new(entity, 3, "zapper".to_owned(), CarrierId::WORLD));

        assert!(!actors.has_vacated_spawn_zones());

        actors.remove(&id);

        assert_eq!(actors.drain_vacated_spawn_zones().collect::<Vec<_>>(), vec![3]);
    }
}
