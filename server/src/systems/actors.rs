use bevy::prelude::*;
use rand::{RngExt, rng, rngs::ThreadRng};

use super::network::broadcast_to_all;
use super::players::generate_player_spawn_position;
use crate::resources::{ActorInfo, ActorMap, MapConfig, PlayerMap};
use common::{
    constants::CHARACTER_FALL_TELEPORT_Y,
    markers::{ActorMarker, PlayerMarker},
    physics::{CharacterVerticalMotion, CollisionWorld},
    protocol::{
        ActorId, ActorKind, CharacterMoveIntent, CharacterMovementState, FaceDirection, Position, SActorMoveIntent,
        SActorTeleport, ServerMessage,
    },
};

const INITIAL_ACTOR_COUNT: u32 = 6;
const ACTOR_MIN_DIRECTION_TIME: f32 = 1.0;
const ACTOR_MAX_DIRECTION_TIME: f32 = 3.5;
const ACTOR_IDLE_CHANCE: f32 = 0.15;

pub fn actor_initial_spawn_system(
    mut commands: Commands,
    mut actors: ResMut<ActorMap>,
    map_config: Res<MapConfig>,
    collision_world: Res<CollisionWorld>,
    players: Query<&Position, With<PlayerMarker>>,
) {
    if !actors.0.is_empty() {
        return;
    }

    let mut occupied_positions: Vec<Position> = players.iter().copied().collect();
    let mut rng = rng();

    for id in 0..INITIAL_ACTOR_COUNT {
        let actor_id = ActorId(id);
        let pos = generate_player_spawn_position(&map_config, &collision_world, &occupied_positions);
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
                CharacterVerticalMotion::default(),
            ))
            .id();

        actors.0.insert(
            actor_id,
            ActorInfo {
                entity,
                kind: ActorKind::Automaton,
                direction_timer: random_direction_time(&mut rng),
            },
        );
    }
}

pub fn actor_ai_system(
    time: Res<Time>,
    players: Res<PlayerMap>,
    mut actors: ResMut<ActorMap>,
    mut query: Query<
        (
            &ActorId,
            &Position,
            &CharacterVerticalMotion,
            &mut CharacterMoveIntent,
            &mut FaceDirection,
        ),
        (With<ActorMarker>, Without<PlayerMarker>),
    >,
) {
    let delta = time.delta_secs();
    let mut rng = rng();

    for (id, pos, motion, mut move_intent, mut face_dir) in &mut query {
        let Some(info) = actors.0.get_mut(id) else {
            continue;
        };

        info.direction_timer -= delta;
        if info.direction_timer > 0.0 {
            continue;
        }

        info.direction_timer = random_direction_time(&mut rng);
        if rng.random_range(0.0..1.0) < ACTOR_IDLE_CHANCE {
            *move_intent = CharacterMoveIntent::Idle;
        } else {
            let direction = rng.random_range(0.0..std::f32::consts::TAU);
            *move_intent = CharacterMoveIntent::Moving { direction };
            face_dir.0 = direction;
        }

        broadcast_actor_move_intent(&players, *id, *pos, *move_intent, motion.0);
    }
}

pub fn actor_fall_recovery_system(
    players: Res<PlayerMap>,
    map_config: Res<MapConfig>,
    collision_world: Res<CollisionWorld>,
    mut query: Query<
        (
            Entity,
            &ActorId,
            &mut Position,
            &mut CharacterVerticalMotion,
            &CharacterMoveIntent,
        ),
        With<ActorMarker>,
    >,
) {
    let fallen: Vec<(Entity, ActorId)> = query
        .iter()
        .filter_map(|(entity, id, pos, _, _)| (pos.y < CHARACTER_FALL_TELEPORT_Y).then_some((entity, *id)))
        .collect();

    if fallen.is_empty() {
        return;
    }

    let occupied_positions: Vec<Position> = query.iter().map(|(_, _, pos, _, _)| *pos).collect();

    for (entity, id) in fallen {
        let teleport_pos = generate_player_spawn_position(&map_config, &collision_world, &occupied_positions);

        if let Ok((_, _, mut pos, mut motion, move_intent)) = query.get_mut(entity) {
            *pos = teleport_pos;
            motion.0 = 0.0;
            broadcast_to_all(
                &players,
                ServerMessage::ActorTeleport(SActorTeleport {
                    id,
                    movement: CharacterMovementState::new(teleport_pos, *move_intent, 0.0),
                }),
            );
        }

        info!("{:?} fell and teleported to {:?}", id, teleport_pos);
    }
}

fn broadcast_actor_move_intent(
    players: &PlayerMap,
    id: ActorId,
    pos: Position,
    move_intent: CharacterMoveIntent,
    vertical_velocity: f32,
) {
    broadcast_to_all(
        players,
        ServerMessage::ActorMoveIntent(SActorMoveIntent {
            id,
            movement: CharacterMovementState::new(pos, move_intent, vertical_velocity),
        }),
    );
}

fn random_direction_time(rng: &mut ThreadRng) -> f32 {
    rng.random_range(ACTOR_MIN_DIRECTION_TIME..=ACTOR_MAX_DIRECTION_TIME)
}
