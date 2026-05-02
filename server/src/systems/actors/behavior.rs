use bevy::prelude::*;
use rand::{RngExt, rng, rngs::ThreadRng};

use crate::{
    constants::{ACTOR_IDLE_CHANCE, ACTOR_MAX_DIRECTION_TIME, ACTOR_MIN_DIRECTION_TIME, ACTOR_VISION_RANGE},
    resources::{ActorMap, PlayerMap},
};
use common::{
    config::GameplayConfig,
    markers::{ActorMarker, PlayerMarker},
    physics::CollisionWorld,
    protocol::{ActorId, ActorMoveIntent, PlayerId, Position},
};

pub fn actor_behavior_system(
    time: Res<Time>,
    players: Res<PlayerMap>,
    collision_world: Res<CollisionWorld>,
    gameplay_config: Res<GameplayConfig>,
    mut actors: ResMut<ActorMap>,
    player_query: Query<(&PlayerId, &Position), With<PlayerMarker>>,
    query: Query<(&ActorId, &Position), (With<ActorMarker>, Without<PlayerMarker>)>,
) {
    let delta = time.delta_secs();
    let mut rng = rng();

    for (id, pos) in &query {
        let Some(info) = actors.0.get_mut(id) else {
            continue;
        };

        if let Some(target_pos) =
            visible_player_position(pos, &players, &player_query, &collision_world, &gameplay_config)
        {
            info.go_to_position = Some(target_pos);
            continue;
        }

        if info.go_to_position.is_some() {
            continue;
        }
        info.direction_timer -= delta;
        if info.direction_timer > 0.0 {
            continue;
        }

        info.direction_timer = random_direction_time(&mut rng);
        if rng.random_range(0.0..1.0) < ACTOR_IDLE_CHANCE {
            info.patrol_intent = ActorMoveIntent::Idle;
        } else {
            let direction = rng.random_range(0.0..std::f32::consts::TAU);
            info.patrol_intent = ActorMoveIntent::Moving {
                direction,
                speed: gameplay_config.characters.actor.patrol_speed,
            };
        }
    }
}

fn visible_player_position(
    actor_pos: &Position,
    players: &PlayerMap,
    player_query: &Query<(&PlayerId, &Position), With<PlayerMarker>>,
    collision_world: &CollisionWorld,
    gameplay_config: &GameplayConfig,
) -> Option<Position> {
    let actor_sight_origin = Vec3::new(
        actor_pos.x,
        actor_pos.y + gameplay_config.characters.actor.eye_height(),
        actor_pos.z,
    );
    let player_physics = gameplay_config.characters.player.physics();

    player_query
        .iter()
        .filter(|(id, _)| players.0.get(id).is_some_and(|info| info.logged_in))
        .filter(|(_, pos)| horizontal_distance_sq(actor_pos, pos) <= ACTOR_VISION_RANGE * ACTOR_VISION_RANGE)
        .filter(|(_, pos)| {
            let player_collider_center = Vec3::new(pos.x, player_physics.collider_center_y(pos.y), pos.z);
            collision_world.line_of_sight_clear(actor_sight_origin, player_collider_center)
        })
        .min_by(|(_, a), (_, b)| horizontal_distance_sq(actor_pos, a).total_cmp(&horizontal_distance_sq(actor_pos, b)))
        .map(|(_, pos)| *pos)
}

fn horizontal_distance_sq(a: &Position, b: &Position) -> f32 {
    let dx = a.x - b.x;
    let dz = a.z - b.z;
    dx.mul_add(dx, dz * dz)
}

fn random_direction_time(rng: &mut ThreadRng) -> f32 {
    rng.random_range(ACTOR_MIN_DIRECTION_TIME..=ACTOR_MAX_DIRECTION_TIME)
}
