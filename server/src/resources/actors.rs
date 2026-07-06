use std::collections::{HashMap, VecDeque};

use bevy::prelude::*;

use common::protocol::{ActorId, ActorMoveIntent, PlayerId, Position};

pub struct ActorInfo {
    pub entity: Entity,
    pub spawn_zone_index: usize,
    pub spawn_kind: String,
    pub direction_timer: f32,
    pub patrol_intent: ActorMoveIntent,
    pub go_to_position: Option<Position>,
    pub go_to_position_is_chase: bool,
    pub is_returning_to_spawn: bool,
    pub return_path: VecDeque<Position>,
    pub chase_reacquire_timer: f32,
    // Net-displacement stall watchdog for any goal-directed or moving-patrol
    // state. The actor re-anchors here whenever it displaces past
    // `STALL_PROGRESS_DISTANCE`; failing to for the state's window means it's
    // wedged against geometry and the state's escape fires. Self-arming;
    // `None` while the current state is exempt (vertical hold, intentional
    // patrol idle).
    pub stall_anchor: Option<Position>,
    pub stall_timer: f32,
    // After a stalled return-to-spawn, suppresses the leash re-trigger so the
    // actor patrols in place first — otherwise the identical (possibly
    // straight-line-fallback) return re-arms next tick and livelocks.
    pub return_retry_timer: f32,
    // After a patrol stall (e.g. perched at a wall-base floor edge where every
    // ledge-aware candidate is rejected), patrol candidates are evaluated
    // ledge-unaware for this long. A genuine fall is recycled by the silent
    // fall-removal + respawn machinery.
    pub patrol_ledge_escape_timer: f32,
    // The heading (yaw) the actor committed to and the time left on that
    // commitment. Movement re-decides only when the timer lapses or the
    // committed heading becomes blocked, so the actor doesn't re-pick (and
    // re-broadcast) a new direction every tick — smooth authoritative motion,
    // no reconciliation snaps, sparse `SActorMoveIntent`. Only the direction is
    // committed; the speed always comes from the current desire (so a chase
    // commit can't carry chase speed into patrol). `None` = decide fresh.
    pub committed_direction: Option<f32>,
    pub commit_secs_left: f32,
    // Player who landed the last projectile damage. Read by
    // `actor_removal_system` when the actor's health hits zero, so the
    // `SActorDeath` broadcast can attribute the kill. Chain-explosion
    // damage doesn't touch this field — those deaths read `None`.
    pub last_damager: Option<PlayerId>,
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

impl ActorSpawner {
    pub fn allocate(&mut self) -> ActorId {
        let id = ActorId(self.next_id);
        self.next_id = self.next_id.wrapping_add(1);
        id
    }
}
