use crate::actors::{ActorInfo, ActorMode};
use common::protocol::{ActorMoveIntent, Position};

pub(super) enum ActorDesire {
    Idle,
    HoldFacing { direction: f32 },
    // `target` is the next waypoint, in the actor's carrier frame.
    Move { intent: ActorMoveIntent, target: Position },
}

// `pos` is in the actor's carrier frame like its route, `world_pos` in the
// world like an engagement's `target_pos`; a translation keeps directions,
// so either pair yields the world heading.
pub(super) fn desired_move(
    info: &ActorInfo,
    pos: &Position,
    world_pos: &Position,
    roam_speed: f32,
    active_speed: f32,
) -> ActorDesire {
    if let Some(route) = &info.route
        && let Some(target) = route.next()
    {
        let speed = if matches!(info.mode, ActorMode::Roam) {
            roam_speed
        } else {
            active_speed
        };
        return ActorDesire::Move {
            intent: ActorMoveIntent::Moving {
                direction: direction_toward(pos, &target),
                speed,
            },
            target,
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
