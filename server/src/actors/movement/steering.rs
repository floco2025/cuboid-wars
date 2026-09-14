use std::f32::consts::TAU;

use crate::actors::{ActorInfo, ActorMode};
use common::{
    math::angle_delta_radians,
    protocol::{ActorMoveIntent, Position},
};

// How fast a walking actor swings its heading, in radians per second.
pub(super) const ACTOR_TURN_RATE: f32 = TAU;

pub(super) enum ActorDesire {
    Idle,
    HoldFacing { direction: f32 },
    // `target` is the next waypoint, in the actor's carrier frame.
    Move { intent: ActorMoveIntent, target: Position },
}

// `pos` is in the actor's carrier frame like its route, `world_pos` in the
// world like an engagement's `target_pos`; a translation keeps directions,
// so either pair yields the world heading. `heading` is where the actor
// faces now; a walk turns from it, a ladder move takes its own direction.
pub(super) fn desired_move(
    info: &ActorInfo,
    pos: &Position,
    world_pos: &Position,
    heading: f32,
    roam_speed: f32,
    active_speed: f32,
    delta: f32,
) -> ActorDesire {
    if let Some(route) = &info.route
        && let Some(target) = route.next()
    {
        let speed = if matches!(info.mode, ActorMode::Roam) {
            roam_speed
        } else {
            active_speed
        };
        let mut intent = target.movement_intent(pos, speed, delta);
        if target.is_walk() {
            intent = steer(intent, heading, delta);
        }
        return ActorDesire::Move {
            intent,
            target: target.position,
        };
    }
    if let ActorMode::Engage { target_pos, .. } = info.mode {
        ActorDesire::HoldFacing {
            direction: direction_toward(world_pos, &target_pos),
        }
    } else {
        ActorDesire::Idle
    }
}

pub(super) fn direction_toward(pos: &Position, target: &Position) -> f32 {
    (target.x - pos.x).atan2(target.z - pos.z)
}

// Turns the heading toward the walk by at most a tick of `ACTOR_TURN_RATE`
// and walks along the turned heading, slower the farther the heading still
// has to come round: past a right angle the actor pivots on the spot. The
// path bends through a corner instead of switching direction between ticks.
pub(super) fn steer(intent: ActorMoveIntent, heading: f32, delta: f32) -> ActorMoveIntent {
    let ActorMoveIntent::Moving { direction, speed } = intent else {
        return intent;
    };
    let error = angle_delta_radians(direction, heading);
    let turn = error.clamp(-ACTOR_TURN_RATE * delta, ACTOR_TURN_RATE * delta);
    ActorMoveIntent::Moving {
        direction: angle_delta_radians(heading + turn, 0.0),
        speed: speed * (error - turn).cos().max(0.0),
    }
}
