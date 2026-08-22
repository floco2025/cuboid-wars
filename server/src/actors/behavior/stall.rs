use bevy::prelude::*;
use rand::Rng;

use crate::{
    actors::{ActorGoal, ActorInfo},
    config::ActorFireConfig,
};
use common::protocol::{ActorMoveIntent, Position};

use super::{
    patrol::{forced_patrol_goal, fresh_patrol_goal},
    tick::BehaviorInputs,
};

pub(super) const STALL_PROGRESS_DISTANCE: f32 = 0.4;
pub(super) const PURSUIT_GIVEUP_NO_PROGRESS_SECS: f32 = 1.5;
pub(super) const CHASE_GIVEUP_NO_PROGRESS_SECS: f32 = 3.0;
pub(super) const RETURN_GIVEUP_NO_PROGRESS_SECS: f32 = 3.0;
pub(super) const PATROL_GIVEUP_NO_PROGRESS_SECS: f32 = 4.0;
pub(super) const RETURN_RETRY_SECS: f32 = 5.0;
pub(super) const PATROL_LEDGE_ESCAPE_SECS: f32 = 1.0;

pub(super) fn transition_stall_recovery(info: &mut ActorInfo, inputs: &BehaviorInputs<'_>, rng: &mut impl Rng) {
    let reached_distance = inputs.kind_config.navigation.go_to_reached_distance;
    let Some(window) = stall_window(
        &info.goal,
        &inputs.pos,
        reached_distance,
        inputs.kind_config.fire.as_ref(),
    ) else {
        info.stall_anchor = None;
        return;
    };
    if tick_stall(info, &inputs.pos, inputs.delta, window) {
        recover_from_stall(info, inputs, rng);
    }
}

pub(super) fn stall_window(
    goal: &ActorGoal,
    pos: &Position,
    reached_distance: f32,
    fire: Option<&ActorFireConfig>,
) -> Option<f32> {
    match goal {
        ActorGoal::Patrol {
            intent: ActorMoveIntent::Idle,
            ..
        } => None,
        ActorGoal::Patrol { .. } => Some(PATROL_GIVEUP_NO_PROGRESS_SECS),
        ActorGoal::Chase { .. } => {
            if goal.chase_hold(pos, reached_distance) {
                None
            } else {
                Some(CHASE_GIVEUP_NO_PROGRESS_SECS)
            }
        }
        ActorGoal::Pursuit { .. } => Some(PURSUIT_GIVEUP_NO_PROGRESS_SECS),
        ActorGoal::Return { .. } => Some(RETURN_GIVEUP_NO_PROGRESS_SECS),
        ActorGoal::Approach { .. } => {
            let standoff = fire.map_or(0.0, |config| config.standoff_distance);
            if goal.approach_hold(pos, standoff) {
                None
            } else {
                Some(CHASE_GIVEUP_NO_PROGRESS_SECS)
            }
        }
        ActorGoal::Fire { .. } => None,
        ActorGoal::Flee { .. } => Some(CHASE_GIVEUP_NO_PROGRESS_SECS),
    }
}

fn recover_from_stall(info: &mut ActorInfo, inputs: &BehaviorInputs<'_>, rng: &mut impl Rng) {
    let pos = inputs.pos;
    info.stall_anchor = None;
    match &info.goal {
        ActorGoal::Chase { .. } => {
            debug!(
                "{}#{} (zone {}) chase stalled at ({:.2},{:.2},{:.2}); giving up for {:.1}s",
                info.spawn_kind,
                inputs.id.0,
                info.spawn_zone_index,
                pos.x,
                pos.y,
                pos.z,
                inputs.kind_config.senses.chase_reacquire_cooldown_secs
            );
            info.chase_reacquire_timer = inputs.kind_config.senses.chase_reacquire_cooldown_secs;
            info.goal = forced_patrol_goal(rng, inputs.patrol_speed, inputs.kind_config);
        }
        ActorGoal::Approach { .. } => {
            debug!(
                "{}#{} (zone {}) approach stalled at ({:.2},{:.2},{:.2}); giving up for {:.1}s",
                info.spawn_kind,
                inputs.id.0,
                info.spawn_zone_index,
                pos.x,
                pos.y,
                pos.z,
                inputs.kind_config.senses.chase_reacquire_cooldown_secs
            );
            info.chase_reacquire_timer = inputs.kind_config.senses.chase_reacquire_cooldown_secs;
            info.goal = forced_patrol_goal(rng, inputs.patrol_speed, inputs.kind_config);
        }
        ActorGoal::Flee { .. } => {
            debug!(
                "{}#{} (zone {}) flee stalled at ({:.2},{:.2},{:.2}); patrolling out the cooldown",
                info.spawn_kind, inputs.id.0, info.spawn_zone_index, pos.x, pos.y, pos.z
            );
            info.goal = forced_patrol_goal(rng, inputs.patrol_speed, inputs.kind_config);
        }
        ActorGoal::Fire { .. } => {}
        ActorGoal::Pursuit { .. } => {
            debug!(
                "{}#{} (zone {}) abandoning unreachable last-seen spot at ({:.2},{:.2},{:.2}); resuming patrol",
                info.spawn_kind, inputs.id.0, info.spawn_zone_index, pos.x, pos.y, pos.z
            );
            info.goal = fresh_patrol_goal();
        }
        ActorGoal::Return { next, path } => {
            warn!(
                "{}#{} (zone {}) return stalled at ({:.2},{:.2},{:.2}) heading to waypoint ({:.2},{:.2},{:.2}) with {} more; patrolling {RETURN_RETRY_SECS}s before retry",
                info.spawn_kind,
                inputs.id.0,
                info.spawn_zone_index,
                pos.x,
                pos.y,
                pos.z,
                next.x,
                next.y,
                next.z,
                path.len()
            );
            info.return_retry_timer = RETURN_RETRY_SECS;
            info.goal = forced_patrol_goal(rng, inputs.patrol_speed, inputs.kind_config);
        }
        ActorGoal::Patrol { .. } => {
            debug!(
                "{}#{} (zone {}) patrol stalled at ({:.2},{:.2},{:.2}); ledge-unaware escape for {PATROL_LEDGE_ESCAPE_SECS}s",
                info.spawn_kind, inputs.id.0, info.spawn_zone_index, pos.x, pos.y, pos.z
            );
            let mut goal = forced_patrol_goal(rng, inputs.patrol_speed, inputs.kind_config);
            if let ActorGoal::Patrol { ledge_escape_timer, .. } = &mut goal {
                *ledge_escape_timer = PATROL_LEDGE_ESCAPE_SECS;
            }
            info.goal = goal;
        }
    }
}

pub(super) fn tick_stall(info: &mut ActorInfo, pos: &Position, delta: f32, window_secs: f32) -> bool {
    let Some(anchor) = info.stall_anchor else {
        info.stall_anchor = Some(*pos);
        info.stall_timer = window_secs;
        return false;
    };
    if anchor.horizontal_distance_sq(pos) > STALL_PROGRESS_DISTANCE * STALL_PROGRESS_DISTANCE {
        info.stall_anchor = Some(*pos);
        info.stall_timer = window_secs;
        return false;
    }
    info.stall_timer -= delta;
    info.stall_timer <= 0.0
}
