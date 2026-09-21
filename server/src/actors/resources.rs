use std::collections::{HashMap, HashSet};

use bevy::prelude::*;

use common::{
    config::ActorGameplayConfig,
    map::Carriers,
    physics::{CharacterMovePlan, CharacterSupport, CharacterVerticalVelocity, ProgressWatchdog},
    protocol::{
        ActorAnchor, ActorBeam, ActorId, ActorMarker, ActorMoveIntent, CarrierId, FaceYaw, Health, PlayerId, Position,
    },
};

use super::navigation::air::FlightState;

// Whether this tick's movement left the actor inside a carrier's geometry;
// written by `apply_actor_moves`, read by `actors_removal_system`.
#[derive(Component, Default)]
pub struct ActorCrushed(pub bool);

// The downward speed this tick's movement landed the actor with, zero when
// it did not land; written by `apply_actor_moves`, read by
// `actors_fall_damage_system`.
#[derive(Component, Default)]
pub struct ActorLanding(pub f32);

// This tick's accepted ground actor moves; written by
// `surface_actors_movement_system`, read by `characters_movement_system`,
// whose flying plans sweep against them.
#[derive(Resource, Default)]
pub struct SurfaceActorMoves(pub Vec<CharacterMovePlan>);

// The kind's body and abilities, resolved once at materialization so
// movement never looks the kind up by name per tick.
#[derive(Component, Clone)]
pub struct ActorCharacter(pub ActorGameplayConfig);

pub type ActorStateQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static Position,
        &'static ActorMoveIntent,
        &'static FaceYaw,
        &'static Health,
        &'static ActorCharacter,
    ),
    With<ActorMarker>,
>;

pub type ActorMotionQuery<'w, 's> =
    Query<'w, 's, (&'static CharacterVerticalVelocity, &'static CharacterSupport), With<ActorMarker>>;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) enum ActorMode {
    #[default]
    Roam,
    Engage {
        target: PlayerId,
        target_pos: Position,
    },
    // A fleeing actor keeps its retreat until the leg ends.
    Evade {
        fleeing: bool,
    },
    ReturnHome,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum BeamState {
    Ready,
    Firing {
        target: PlayerId,
        started_tick: u32,
        remaining_secs: f32,
    },
    Cooldown {
        remaining_secs: f32,
    },
}

impl BeamState {
    pub(crate) fn target(&self) -> Option<PlayerId> {
        match *self {
            Self::Firing { target, .. } => Some(target),
            Self::Ready | Self::Cooldown { .. } => None,
        }
    }

    pub(crate) fn snapshot(&self) -> Option<ActorBeam> {
        match *self {
            Self::Firing {
                target,
                started_tick,
                remaining_secs,
            } => Some(ActorBeam {
                target,
                started_tick,
                remaining_secs,
            }),
            Self::Ready | Self::Cooldown { .. } => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct AwarePlayer {
    pub(crate) id: PlayerId,
    pub(crate) pos: Position,
    // Where the player stood when last seen, in that carrier's frame, so a
    // remembered sighting rides with its platform.
    pub(crate) carrier: CarrierId,
    pub(crate) carrier_pos: Position,
    pub(crate) support: CharacterSupport,
    pub(crate) visible: bool,
    pub(crate) forget_remaining_secs: f32,
}

// Ground routes use the supporting carrier; spawn ownership is the zone index.
pub struct ActorInfo {
    pub entity: Entity,
    pub spawn_zone_index: usize,
    pub spawn_kind: String,
    pub carrier: CarrierId,
    pub anchor: Option<ActorAnchor>,
    pub(crate) flight: Option<FlightState>,
    pub(crate) mode: ActorMode,
    pub(crate) beam: BeamState,
    pub(crate) awareness: Vec<AwarePlayer>,
    pub(crate) decision_timer: f32,
    pub(crate) watchdog: ProgressWatchdog,
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
            anchor: None,
            flight: None,
            mode: ActorMode::Roam,
            beam: BeamState::Ready,
            awareness: Vec::new(),
            decision_timer: 0.0,
            watchdog: ProgressWatchdog::default(),
            last_damager: None,
        }
    }
}

#[derive(Resource, Default)]
pub struct ActorMap {
    pub peaceful: bool,
    entries: HashMap<ActorId, ActorInfo>,
    // One entry per removed actor: simultaneous deaths in a zone each owe its countdown.
    vacated_spawn_zones: Vec<usize>,
}

impl ActorMap {
    pub fn set_peaceful(&mut self, peaceful: bool) {
        if self.peaceful == peaceful {
            return;
        }
        self.peaceful = peaceful;
        for info in self.entries.values_mut() {
            info.decision_timer = 0.0;
            if peaceful {
                info.beam = BeamState::Ready;
                info.awareness.clear();
                info.mode = ActorMode::ReturnHome;
            }
        }
    }

    pub fn insert(&mut self, id: ActorId, info: ActorInfo) -> Option<ActorInfo> {
        self.entries.insert(id, info)
    }

    // Forget every actor and vacated zone; `peaceful` is an admin setting, not actor state.
    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.vacated_spawn_zones.clear();
    }

    pub fn remove(&mut self, id: &ActorId) -> Option<ActorInfo> {
        let info = self.entries.remove(id)?;
        self.vacated_spawn_zones.push(info.spawn_zone_index);
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
        self.vacated_spawn_zones.drain(..)
    }

    pub(crate) fn forget_vacated_spawn_zones(&mut self) {
        self.vacated_spawn_zones.clear();
    }

    #[must_use]
    pub fn has_vacated_spawn_zones(&self) -> bool {
        !self.vacated_spawn_zones.is_empty()
    }
}

// Per-zone slot accounting, keyed by zone index. A zone's population is its
// target for the logged-in players; each vacated slot waits its own countdown
// and the deficit fills at once, so a join spawns only its new slots and a
// rejoin never revives a permanent kill or skips a countdown.
#[derive(Resource, Default)]
pub struct ActorSpawner {
    pub next_id: u32,
    // The remaining delay of every vacated slot; `None` never refills.
    pub(crate) refills: HashMap<usize, Vec<Option<f32>>>,
    // Zones whose fill found no clear spot, warned once until a spawn succeeds.
    pub(crate) blocked: HashSet<usize>,
}

// A spawn that has been decided (id, spot, and heading reserved) but whose
// beam-in warning window hasn't elapsed. The actor entity doesn't exist yet —
// clients render a ghost from the snapshot's `spawning_actors` list. Counts
// toward the zone quota until materialized or canceled because its spot is blocked.
// `pos` is in the zone's carrier frame, so the spot rides
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
#[path = "tests/resources.rs"]
mod tests;
