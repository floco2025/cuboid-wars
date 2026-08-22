use rand::{Rng, RngExt};
use std::f32::consts::TAU;

use crate::{actors::ActorGoal, config::ActorKindServerConfig};
use common::protocol::ActorMoveIntent;

// Entry path for non-escape transitions into patrol (pursuit arrival/stall,
// return completion): the zero timer re-rolls a heading — honoring
// `idle_probability` — in the same behavior tick.
pub(crate) fn fresh_patrol_goal() -> ActorGoal {
    ActorGoal::Patrol {
        intent: ActorMoveIntent::Idle,
        direction_timer: 0.0,
        ledge_escape_timer: 0.0,
    }
}

// Post-stall escape entry: the actor must actually move, so the roll
// bypasses `idle_probability`.
pub(crate) fn forced_patrol_goal(
    rng: &mut impl Rng,
    patrol_speed: f32,
    kind_server_config: &ActorKindServerConfig,
) -> ActorGoal {
    ActorGoal::Patrol {
        intent: random_patrol_move_intent(rng, patrol_speed),
        direction_timer: random_direction_time(rng, kind_server_config),
        ledge_escape_timer: 0.0,
    }
}

pub(crate) fn random_patrol_intent(rng: &mut impl Rng, patrol_speed: f32, idle_probability: f32) -> ActorMoveIntent {
    if rng.random_range(0.0..1.0) < idle_probability {
        ActorMoveIntent::Idle
    } else {
        random_patrol_move_intent(rng, patrol_speed)
    }
}

pub(crate) fn random_patrol_move_intent(rng: &mut impl Rng, patrol_speed: f32) -> ActorMoveIntent {
    ActorMoveIntent::Moving {
        direction: rng.random_range(0.0..TAU),
        speed: patrol_speed,
    }
}

pub(crate) fn random_direction_time(rng: &mut impl Rng, kind_server_config: &ActorKindServerConfig) -> f32 {
    rng.random_range(kind_server_config.patrol.min_direction_secs..=kind_server_config.patrol.max_direction_secs)
}
