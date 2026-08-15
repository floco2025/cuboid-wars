use std::collections::{HashMap, VecDeque};

use bevy::prelude::*;

use common::protocol::{ActorId, ActorMoveIntent, PlayerId, Position};

// The hold (see `ActorGoal::chase_hold`) is for a chase target that's
// genuinely unreachable vertically — a player on a ledge above, or tucked
// below an overhang. Below the player jump apex so a hopping same-height
// player doesn't flicker the hold; a same-height target within reach (e.g.
// through a thin wall) keeps the chase pressing so the stall watchdog can
// end it. Without a wall, contact detonation fires well before this distance.
const CHASE_HOLD_MIN_VERTICAL_GAP: f32 = 1.0;

// What the actor is currently trying to do. Behavior owns every transition;
// movement only reads it (via `desired_move`) and never mutates it.
#[derive(Debug, Clone, PartialEq)]
pub enum ActorGoal {
    Patrol {
        intent: ActorMoveIntent,
        // Time until the next random heading re-roll.
        direction_timer: f32,
        // After a patrol stall (e.g. perched at a wall-base floor edge where
        // every strict candidate is rejected), candidates are evaluated
        // ledge-unaware for this long. A genuine fall is recycled by the
        // silent fall-removal + respawn machinery.
        ledge_escape_timer: f32,
    },
    // Pressing toward a visible player's live position. Never "arrives" —
    // ends via contact detonation, the vertical hold, demotion to `Pursuit`,
    // the leash, or the stall watchdog. Contact kinds only.
    Chase {
        target: Position,
    },
    // Demoted chase: one-shot walk toward the player's last-seen spot.
    Pursuit {
        last_seen: Position,
    },
    // Waypoint walk back to the spawn zone.
    Return {
        next: Position,
        path: VecDeque<Position>,
    },
    // Laser kinds' replacement for `Chase`: press toward the target but hold
    // at the standoff distance. `target_pos` is refreshed each tick (movement
    // reads the goal without a player query, like `Chase`).
    Approach {
        target: PlayerId,
        target_pos: Position,
    },
    // Mid-burst: stand still, yaw tracks the target, the beam damage system
    // burns whoever the beam reaches. The target is locked for the burst.
    Fire {
        target: PlayerId,
        target_pos: Position,
        remaining_secs: f32,
    },
    // Post-burst hide: run directly away from the threat until the re-fire
    // cooldown (`ActorInfo::fire_cooldown_timer`) lapses.
    Flee {
        threat: Position,
    },
}

impl ActorGoal {
    // Where the actor is headed, for plan ordering and steering. Patrol has
    // no target (wander).
    #[must_use]
    pub fn target_position(&self) -> Option<Position> {
        match self {
            Self::Patrol { .. } | Self::Fire { .. } | Self::Flee { .. } => None,
            Self::Chase { target } => Some(*target),
            Self::Pursuit { last_seen } => Some(*last_seen),
            Self::Return { next, .. } => Some(*next),
            Self::Approach { target_pos, .. } => Some(*target_pos),
        }
    }

    // A chaser that has closed the horizontal gap to a vertically-unreachable
    // target holds position and facing rather than arriving — otherwise it
    // reverts to patrol, drifts, gets re-acquired, and flip-flops its heading
    // every tick (the jitter). Only `Chase` holds; returners must arrive.
    #[must_use]
    pub fn chase_hold(&self, pos: &Position, reached_distance: f32) -> bool {
        let Self::Chase { target } = self else {
            return false;
        };
        pos.horizontal_distance_sq(target) <= reached_distance * reached_distance
            && (target.y - pos.y).abs() >= CHASE_HOLD_MIN_VERTICAL_GAP
    }

    // An approacher inside its standoff distance holds position (and facing)
    // instead of pressing to touch — the standoff is the point of the state.
    // Unlike `chase_hold` there is no vertical gate: horizontal arrival is
    // enough, which also absorbs the player-directly-above jitter case.
    #[must_use]
    pub fn approach_hold(&self, pos: &Position, standoff_distance: f32) -> bool {
        let Self::Approach { target_pos, .. } = self else {
            return false;
        };
        pos.horizontal_distance_sq(target_pos) <= standoff_distance * standoff_distance
    }
}

pub struct ActorInfo {
    pub entity: Entity,
    pub spawn_zone_index: usize,
    pub spawn_kind: String,
    pub goal: ActorGoal,
    pub chase_reacquire_timer: f32,
    // Net-displacement stall watchdog for any goal-directed or moving-patrol
    // state. The actor re-anchors here whenever it displaces past
    // `STALL_PROGRESS_DISTANCE`; failing to for the goal's window means it's
    // wedged against geometry and the goal's escape fires. Self-arming;
    // `None` while the current goal is exempt (vertical hold, intentional
    // patrol idle).
    pub stall_anchor: Option<Position>,
    pub stall_timer: f32,
    // After a stalled return-to-spawn, suppresses the leash re-trigger so the
    // actor patrols in place first — otherwise the identical (possibly
    // straight-line-fallback) return re-arms next tick and livelocks. Armed
    // by a Return escape but consumed while patrolling, so it can't live
    // inside the `Return` variant.
    pub return_retry_timer: f32,
    // The heading (yaw) the actor committed to and the time left on that
    // commitment. Movement re-decides only when the timer lapses or the
    // committed heading becomes blocked, so the actor doesn't re-pick (and
    // re-broadcast) a new direction every tick — smooth authoritative motion,
    // no reconciliation snaps, sparse `SActorMoveIntent`. Only the direction is
    // committed; the speed always comes from the current desire (so a chase
    // commit can't carry chase speed into patrol). `None` = decide fresh.
    pub committed_direction: Option<f32>,
    pub commit_secs_left: f32,
    // Re-fire lockout, armed to `fire.cooldown_secs` when a burst ends and
    // ticked down every tick regardless of goal — a leash-preempted flee
    // (Flee → Return) must not erase it, so it can't live inside the `Flee`
    // variant (same reasoning as `return_retry_timer`). `Flee` ends and
    // firing re-arms only once it lapses.
    pub fire_cooldown_timer: f32,
    // Throttles the beam-victim `SPlayerHit` cue while burst damage applies.
    pub fire_hit_cue_timer: f32,
    // Player who landed the last projectile damage. Read by
    // `actor_removal_system` when the actor's health hits zero, so the
    // `SActorDeath` broadcast can attribute the kill. Chain-explosion
    // damage doesn't touch this field — those deaths read `None`.
    pub last_damager: Option<PlayerId>,
}

impl ActorInfo {
    #[must_use]
    pub fn new(entity: Entity, spawn_zone_index: usize, spawn_kind: String, goal: ActorGoal) -> Self {
        Self {
            entity,
            spawn_zone_index,
            spawn_kind,
            goal,
            chase_reacquire_timer: 0.0,
            stall_anchor: None,
            stall_timer: 0.0,
            return_retry_timer: 0.0,
            committed_direction: None,
            commit_secs_left: 0.0,
            fire_cooldown_timer: 0.0,
            fire_hit_cue_timer: 0.0,
            last_damager: None,
        }
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
    pub face_dir: f32,
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
