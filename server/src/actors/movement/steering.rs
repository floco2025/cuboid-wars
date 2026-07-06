use crate::resources::ActorGoal;
use common::{
    math::angle_delta_radians,
    protocol::{ActorMoveIntent, Position},
};

use super::context::StepPolicy;

// What movement should do this tick — a pure read of the goal. Behavior owns
// every goal transition; this layer never writes it.
pub(super) enum ActorDesire {
    // Patrol idle, or a chase holding under a vertically-unreachable player:
    // stand still, keep facing.
    Idle,
    Move {
        intent: ActorMoveIntent,
        policy: StepPolicy,
    },
}

pub(super) fn desired_move(goal: &ActorGoal, pos: &Position, chase_speed: f32, reached_distance: f32) -> ActorDesire {
    match goal {
        ActorGoal::Patrol {
            intent: ActorMoveIntent::Idle,
            ..
        } => ActorDesire::Idle,
        ActorGoal::Patrol {
            intent,
            ledge_escape_timer,
            ..
        } => ActorDesire::Move {
            intent: *intent,
            // Never off an edge — except during the post-stall escape window
            // (perched at a wall-base edge where every strict candidate is
            // rejected).
            policy: if *ledge_escape_timer > 0.0 {
                StepPolicy::Pursue
            } else {
                StepPolicy::Strict
            },
        },
        ActorGoal::Chase { .. } if goal.chase_hold(pos, reached_distance) => ActorDesire::Idle,
        // A chase presses toward the live target and never "arrives"; a
        // pursuit or return heads at its spot until behavior arrives it.
        // All three follow targets off ledges.
        ActorGoal::Chase { target }
        | ActorGoal::Pursuit { last_seen: target }
        | ActorGoal::Return { next: target, .. } => ActorDesire::Move {
            intent: ActorMoveIntent::Moving {
                direction: direction_toward(pos, target),
                speed: chase_speed,
            },
            policy: StepPolicy::Pursue,
        },
    }
}

pub(super) fn direction_toward(pos: &Position, target: &Position) -> f32 {
    let dx = target.x - pos.x;
    let dz = target.z - pos.z;
    dx.atan2(dz)
}

// Smallest absolute angle between two headings, in `[0, PI]`.
pub fn angular_distance(a: f32, b: f32) -> f32 {
    angle_delta_radians(a, b).abs()
}
