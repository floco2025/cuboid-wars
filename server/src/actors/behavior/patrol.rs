use rand::{RngExt, rngs::ThreadRng};

use crate::config::ActorKindServerConfig;
use common::protocol::ActorMoveIntent;

pub(crate) fn random_patrol_intent(rng: &mut ThreadRng, patrol_speed: f32, idle_probability: f32) -> ActorMoveIntent {
    if rng.random_range(0.0..1.0) < idle_probability {
        ActorMoveIntent::Idle
    } else {
        random_patrol_move_intent(rng, patrol_speed)
    }
}

pub(crate) fn random_patrol_move_intent(rng: &mut ThreadRng, patrol_speed: f32) -> ActorMoveIntent {
    ActorMoveIntent::Moving {
        direction: rng.random_range(0.0..std::f32::consts::TAU),
        speed: patrol_speed,
    }
}

pub(crate) fn random_direction_time(rng: &mut ThreadRng, kind_server_config: &ActorKindServerConfig) -> f32 {
    rng.random_range(kind_server_config.min_direction_time..=kind_server_config.max_direction_time)
}
