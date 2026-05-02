use bevy::prelude::*;
use rand::{RngExt, rng, rngs::ThreadRng};

use super::network::broadcast_to_all;
use crate::{
    constants::{
        ACTOR_IDLE_CHANCE, ACTOR_INITIAL_COUNT, ACTOR_MAX_DIRECTION_TIME, ACTOR_MIN_DIRECTION_TIME, ACTOR_VISION_RANGE,
    },
    resources::{ActorInfo, ActorMap, MapConfig, PlayerMap},
    systems::players::generate_character_spawn_position,
};
use common::{
    config::GameplayConfig,
    constants::CHARACTER_FALL_TELEPORT_Y,
    markers::{ActorMarker, PlayerMarker},
    physics::{CharacterVerticalMotion, CollisionWorld},
    protocol::{
        ActorId, ActorKind, CharacterMoveIntent, CharacterMovementState, FaceDirection, PlayerId, Position,
        SActorMoveIntent, SActorTeleport, ServerMessage,
    },
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
                CharacterVerticalMotion::default(),
            ))
            .id();

        actors.0.insert(
            actor_id,
            ActorInfo {
                entity,
                kind: ActorKind::Automaton,
                direction_timer: random_direction_time(&mut rng),
                go_to_position: None,
                avoidance_side: random_avoidance_side(&mut rng),
                avoidance_timer: 0.0,
                intent_change_cooldown: 0.0,
            },
        );
    }
}

pub fn actor_ai_system(
    time: Res<Time>,
    players: Res<PlayerMap>,
    collision_world: Res<CollisionWorld>,
    gameplay_config: Res<GameplayConfig>,
    mut actors: ResMut<ActorMap>,
    player_query: Query<(&PlayerId, &Position), With<PlayerMarker>>,
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

        info.avoidance_timer = (info.avoidance_timer - delta).max(0.0);
        info.intent_change_cooldown = (info.intent_change_cooldown - delta).max(0.0);
        if let Some(target_pos) = visible_player_position(
            pos,
            &players,
            &player_query,
            &collision_world,
            gameplay_config.characters.player.eye_height(),
        ) {
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
            *move_intent = CharacterMoveIntent::Idle;
        } else {
            let direction = rng.random_range(0.0..std::f32::consts::TAU);
            *move_intent = CharacterMoveIntent::Moving { direction };
            face_dir.0 = direction;
        }

        broadcast_actor_move_intent(&players, *id, *pos, *move_intent, motion.0);
    }
}

fn visible_player_position(
    actor_pos: &Position,
    players: &PlayerMap,
    player_query: &Query<(&PlayerId, &Position), With<PlayerMarker>>,
    collision_world: &CollisionWorld,
    sight_height: f32,
) -> Option<Position> {
    player_query
        .iter()
        .filter(|(id, _)| players.0.get(id).is_some_and(|info| info.logged_in))
        .filter(|(_, pos)| horizontal_distance_sq(actor_pos, pos) <= ACTOR_VISION_RANGE * ACTOR_VISION_RANGE)
        .filter(|(_, pos)| collision_world.character_line_of_sight_clear(actor_pos, pos, sight_height))
        .min_by(|(_, a), (_, b)| horizontal_distance_sq(actor_pos, a).total_cmp(&horizontal_distance_sq(actor_pos, b)))
        .map(|(_, pos)| *pos)
}

fn horizontal_distance_sq(a: &Position, b: &Position) -> f32 {
    let dx = a.x - b.x;
    let dz = a.z - b.z;
    dx.mul_add(dx, dz * dz)
}

pub fn actor_fall_recovery_system(
    players: Res<PlayerMap>,
    map_config: Res<MapConfig>,
    collision_world: Res<CollisionWorld>,
    gameplay_config: Res<GameplayConfig>,
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
        let teleport_pos = generate_character_spawn_position(
            &map_config,
            &collision_world,
            &occupied_positions,
            gameplay_config.characters.actor.physics(),
        );

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

fn random_avoidance_side(rng: &mut ThreadRng) -> f32 {
    if rng.random_bool(0.5) { 1.0 } else { -1.0 }
}
