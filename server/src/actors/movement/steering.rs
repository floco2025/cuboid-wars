use crate::{actors::ActorGoal, config::ActorFireConfig};
use common::{
    math::angle_delta_radians,
    protocol::{ActorMoveIntent, Position},
};

use super::context::StepPolicy;

// What movement should do this tick — a pure read of the goal. Behavior owns
// every goal transition; this layer never writes it.
pub(super) enum ActorDesire {
    // Patrol idle, an approach holding at standoff, or a chase holding under
    // a vertically-unreachable player: stand still, keep facing.
    Idle,
    // Mid-burst: stand still but track the target with yaw.
    HoldFacing {
        direction: f32,
    },
    Move {
        intent: ActorMoveIntent,
        policy: StepPolicy,
    },
}

pub(super) fn desired_move(
    goal: &ActorGoal,
    pos: &Position,
    chase_speed: f32,
    reached_distance: f32,
    fire: Option<&ActorFireConfig>,
) -> ActorDesire {
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
        ActorGoal::Approach { .. } if fire.is_some_and(|f| goal.approach_hold(pos, f.standoff_distance)) => {
            ActorDesire::Idle
        }
        // A chase or approach presses toward the live target and never
        // "arrives"; a pursuit or return heads at its spot until behavior
        // arrives it. All four follow targets off ledges.
        ActorGoal::Chase { target }
        | ActorGoal::Pursuit { last_seen: target }
        | ActorGoal::Return { next: target, .. }
        | ActorGoal::Approach { target_pos: target, .. } => ActorDesire::Move {
            intent: ActorMoveIntent::Moving {
                direction: direction_toward(pos, target),
                speed: chase_speed,
            },
            policy: StepPolicy::Pursue,
        },
        ActorGoal::Fire { target_pos, .. } => ActorDesire::HoldFacing {
            direction: direction_toward(pos, target_pos),
        },
        // Reversed arguments = directly away from the threat. Strict policy:
        // there's no target worth following off a ledge, and the avoidance
        // fan already slides the flee around corners; a dead end is ended by
        // the stall watchdog.
        ActorGoal::Flee { threat } => ActorDesire::Move {
            intent: ActorMoveIntent::Moving {
                direction: direction_toward(threat, pos),
                speed: chase_speed,
            },
            policy: StepPolicy::Strict,
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
