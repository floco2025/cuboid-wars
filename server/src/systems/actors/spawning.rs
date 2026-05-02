use bevy::prelude::*;
use rand::{RngExt, rng, rngs::ThreadRng};

use crate::{
    constants::{
        ACTOR_INITIAL_COUNT, ACTOR_MAX_DIRECTION_TIME, ACTOR_MIN_DIRECTION_TIME, ACTOR_MOVE_INTENT_SEND_COOLDOWN,
    },
    resources::{ActorInfo, ActorMap, MapConfig},
    systems::characters::generate_character_spawn_position,
};
use common::{
    config::GameplayConfig,
    markers::{ActorMarker, PlayerMarker},
    physics::{CharacterVerticalVelocity, CollisionWorld},
    protocol::{ActorId, ActorKind, CharacterMoveIntent, FaceDirection, Position},
};

pub fn actor_initial_spawn_system(
    mut commands: Commands,
    mut actors: ResMut<ActorMap>,
    map_config: Res<MapConfig>,
    collision_world: Res<CollisionWorld>,
    gameplay_config: Res<GameplayConfig>,
    players: Query<&Position, With<PlayerMarker>>,
) {
    if !actors.0.is_empty() {
        return;
    }

    let mut occupied_positions: Vec<Position> = players.iter().copied().collect();
    let mut rng = rng();

    for id in 0..ACTOR_INITIAL_COUNT {
        let actor_id = ActorId(id);
        let pos = generate_character_spawn_position(
            &map_config,
            &collision_world,
            &occupied_positions,
            gameplay_config.characters.actor.physics(),
        );
        occupied_positions.push(pos);

        let direction = rng.random_range(0.0..std::f32::consts::TAU);
        let move_intent = CharacterMoveIntent::Moving { direction };
        let entity = commands
            .spawn((
                ActorMarker,
                actor_id,
                pos,
                move_intent,
                FaceDirection(direction),
                CharacterVerticalVelocity::default(),
            ))
            .id();

        actors.0.insert(
            actor_id,
            ActorInfo {
                entity,
                kind: ActorKind::Automaton,
                direction_timer: random_direction_time(&mut rng),
                patrol_intent: move_intent,
                go_to_position: None,
                wall_avoidance_direction: None,
                last_broadcast_move_intent: move_intent,
                move_intent_send_timer: ACTOR_MOVE_INTENT_SEND_COOLDOWN,
            },
        );
    }
}

fn random_direction_time(rng: &mut ThreadRng) -> f32 {
    rng.random_range(ACTOR_MIN_DIRECTION_TIME..=ACTOR_MAX_DIRECTION_TIME)
}
