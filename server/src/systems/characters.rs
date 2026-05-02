use bevy::prelude::*;
use rand::{RngExt, rng, rngs::ThreadRng};

use super::network::broadcast_to_all;
use crate::{
    constants::{
        ACTOR_AVOIDANCE_TIME, ACTOR_GO_TO_REACHED_DISTANCE, ACTOR_MOVE_INTENT_DIR_CHANGE_THRESHOLD,
        ACTOR_MOVE_INTENT_SEND_COOLDOWN, ACTOR_TURN_SPEED,
    },
    resources::{ActorInfo, ActorMap, PlayerInfo, PlayerMap},
};
use common::{
    config::{CharacterPhysicsConfig, GameplayConfig},
    constants::PHYSICS_EPSILON,
    markers::{ActorMarker, PlayerMarker},
    physics::{
        CharacterVerticalMotion, CollisionWorld, PlannedCharacterMove, overlapping_character,
        planned_character_moves_intersect, step_character_movement,
    },
    protocol::{
        ActorId, CharacterMoveIntent, CharacterMovementState, FaceDirection, PlayerId, Position, SActorMoveIntent,
        ServerMessage,
    },
};

type PlayerMovementQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut Position,
        &'static mut CharacterVerticalMotion,
        &'static CharacterMoveIntent,
        &'static PlayerId,
    ),
    (With<PlayerMarker>, Without<ActorMarker>),
>;

type ActorMovementQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ActorId,
        &'static mut Position,
        &'static mut CharacterVerticalMotion,
        &'static mut CharacterMoveIntent,
        &'static mut FaceDirection,
    ),
    (With<ActorMarker>, Without<PlayerMarker>),
>;

pub fn characters_movement_system(
    time: Res<Time>,
    collision_world: Res<CollisionWorld>,
    gameplay_config: Res<GameplayConfig>,
    players: Res<PlayerMap>,
    mut actors: ResMut<ActorMap>,
    mut player_query: PlayerMovementQuery,
    mut actor_query: ActorMovementQuery,
) {
    let delta = time.delta_secs();
    let mut planned_moves = Vec::new();
    let actor_starts: Vec<(Entity, Position)> = actor_query
        .iter()
        .map(|(entity, _, pos, _, _, _)| (entity, *pos))
        .collect();

    plan_player_moves(
        delta,
        &collision_world,
        &gameplay_config,
        &players,
        &player_query,
        &mut planned_moves,
    );
    plan_actor_moves(
        delta,
        &collision_world,
        &gameplay_config,
        &players,
        &mut actors,
        &actor_starts,
        &mut actor_query,
        &mut planned_moves,
    );
    apply_player_moves(&mut player_query, &planned_moves);
    apply_actor_moves(delta, &players, &mut actors, &mut actor_query, &planned_moves);
}

fn plan_player_moves(
    delta: f32,
    collision_world: &CollisionWorld,
    gameplay_config: &GameplayConfig,
    players: &PlayerMap,
    query: &PlayerMovementQuery,
    planned_moves: &mut Vec<PlannedCharacterMove>,
) {
    let player_config = &gameplay_config.characters.player;
    let player_physics = player_config.physics();
    for (entity, pos, motion, move_intent, player_id) in query.iter() {
        let is_stunned = players.0.get(player_id).is_some_and(|info| info.stun_timer > 0.0);
        let has_speed_power_up = players.0.get(player_id).is_some_and(PlayerInfo::has_speed);
        let velocity = move_intent.to_player_horizontal_velocity(player_config.speed, has_speed_power_up);
        let velocity_sq = velocity.x.mul_add(velocity.x, velocity.z * velocity.z);
        let is_standing_still = velocity_sq < PHYSICS_EPSILON * PHYSICS_EPSILON;
        let suppress_horizontal = is_stunned || is_standing_still;

        let target_xz = if suppress_horizontal {
            *pos
        } else {
            Position {
                x: velocity.x.mul_add(delta, pos.x),
                y: pos.y,
                z: velocity.z.mul_add(delta, pos.z),
            }
        };

        let has_phasing = players.0.get(player_id).is_some_and(PlayerInfo::has_phasing);
        let step = step_character_movement(
            pos,
            motion.0,
            collision_world,
            has_phasing,
            player_physics,
            target_xz.x,
            target_xz.z,
            delta,
        );

        planned_moves.push(PlannedCharacterMove {
            entity,
            start: *pos,
            target: step.position,
            target_vertical_velocity: step.vertical_velocity,
            physics: player_physics,
            blocked: step.blocked,
        });
    }
}

fn plan_actor_moves(
    delta: f32,
    collision_world: &CollisionWorld,
    gameplay_config: &GameplayConfig,
    players: &PlayerMap,
    actors: &mut ActorMap,
    actor_starts: &[(Entity, Position)],
    query: &mut ActorMovementQuery,
    planned_moves: &mut Vec<PlannedCharacterMove>,
) {
    let actor_config = &gameplay_config.characters.actor;
    let actor_physics = actor_config.physics();
    let mut rng = rng();
    for (entity, id, pos, motion, mut move_intent, mut face_dir) in query.iter_mut() {
        let Some(info) = actors.0.get_mut(id) else {
            continue;
        };
        info.move_intent_send_timer += delta;
        let current_intent = *move_intent;
        let desired_intent = actor_desired_intent(info, &pos).unwrap_or(info.patrol_intent);
        let (selected_intent, step, used_avoidance) = select_actor_move(
            entity,
            &pos,
            motion.0,
            current_intent,
            face_dir.0,
            desired_intent,
            info,
            actor_config.speed,
            actor_physics,
            delta,
            collision_world,
            planned_moves,
            actor_starts,
            &mut rng,
        );

        if used_avoidance {
            info.avoidance_timer = ACTOR_AVOIDANCE_TIME;
        }
        *move_intent = selected_intent;
        if let Some(direction) = selected_intent.direction() {
            face_dir.0 = direction;
        }
        maybe_broadcast_actor_move_intent(players, *id, *pos, selected_intent, motion.0, info);

        planned_moves.push(PlannedCharacterMove {
            entity,
            start: *pos,
            target: step.position,
            target_vertical_velocity: step.vertical_velocity,
            physics: actor_physics,
            blocked: step.blocked,
        });
    }
}

fn actor_desired_intent(info: &mut ActorInfo, pos: &Position) -> Option<CharacterMoveIntent> {
    let target_pos = info.go_to_position?;
    if horizontal_distance_sq(pos, &target_pos) <= ACTOR_GO_TO_REACHED_DISTANCE * ACTOR_GO_TO_REACHED_DISTANCE {
        info.go_to_position = None;
        return None;
    }

    Some(CharacterMoveIntent::Moving {
        direction: direction_toward(pos, &target_pos),
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "Candidate testing needs the same movement context as normal actor planning"
)]
fn select_actor_move(
    entity: Entity,
    pos: &Position,
    vertical_velocity: f32,
    current_intent: CharacterMoveIntent,
    face_direction: f32,
    desired_intent: CharacterMoveIntent,
    info: &mut ActorInfo,
    actor_speed: f32,
    actor_physics: CharacterPhysicsConfig,
    delta: f32,
    collision_world: &CollisionWorld,
    planned_moves: &[PlannedCharacterMove],
    actor_starts: &[(Entity, Position)],
    rng: &mut ThreadRng,
) -> (CharacterMoveIntent, common::physics::CharacterMovementResult, bool) {
    let Some(direction) = desired_intent.direction() else {
        let selected_intent = CharacterMoveIntent::Idle;
        let step = step_actor_move(
            pos,
            vertical_velocity,
            selected_intent,
            actor_speed,
            actor_physics,
            delta,
            collision_world,
        );
        return (selected_intent, step, false);
    };

    if info.avoidance_timer <= 0.0 {
        info.avoidance_side = random_avoidance_side(rng);
    }

    let mut fallback = None;
    for (index, candidate_direction) in steering_directions(direction, info.avoidance_side)
        .into_iter()
        .enumerate()
    {
        let candidate_intent = CharacterMoveIntent::Moving {
            direction: candidate_direction,
        };
        let selected_intent = turn_limited_actor_intent(current_intent, face_direction, candidate_intent, delta);
        let step = step_actor_move(
            pos,
            vertical_velocity,
            selected_intent,
            actor_speed,
            actor_physics,
            delta,
            collision_world,
        );
        let planned_move = PlannedCharacterMove {
            entity,
            start: *pos,
            target: step.position,
            target_vertical_velocity: step.vertical_velocity,
            physics: actor_physics,
            blocked: step.blocked,
        };
        let blocked = step.blocked || planned_move_overlaps_character(&planned_move, planned_moves, actor_starts);
        if index == 0 {
            fallback = Some((selected_intent, step));
        }
        if !blocked {
            return (selected_intent, step, index != 0);
        }
    }

    let (intent, step) = fallback.expect("steering directions always include direct movement");
    (intent, step, false)
}

fn step_actor_move(
    pos: &Position,
    vertical_velocity: f32,
    move_intent: CharacterMoveIntent,
    actor_speed: f32,
    actor_physics: CharacterPhysicsConfig,
    delta: f32,
    collision_world: &CollisionWorld,
) -> common::physics::CharacterMovementResult {
    let velocity = move_intent.to_horizontal_velocity(actor_speed);
    let target_x = velocity.x.mul_add(delta, pos.x);
    let target_z = velocity.z.mul_add(delta, pos.z);
    step_character_movement(
        pos,
        vertical_velocity,
        collision_world,
        false,
        actor_physics,
        target_x,
        target_z,
        delta,
    )
}

fn planned_move_overlaps_character(
    candidate: &PlannedCharacterMove,
    planned_moves: &[PlannedCharacterMove],
    actor_starts: &[(Entity, Position)],
) -> bool {
    if overlapping_character(candidate, planned_moves).is_some() {
        return true;
    }

    actor_starts.iter().any(|(entity, pos)| {
        if *entity == candidate.entity {
            return false;
        }
        let stationary_actor = PlannedCharacterMove {
            entity: *entity,
            start: *pos,
            target: *pos,
            target_vertical_velocity: 0.0,
            physics: candidate.physics,
            blocked: false,
        };
        planned_character_moves_intersect(candidate, &stationary_actor)
    })
}

fn steering_directions(direction: f32, side: f32) -> [f32; 7] {
    [
        direction,
        direction + side * 20.0_f32.to_radians(),
        direction + side * 45.0_f32.to_radians(),
        direction + side * 90.0_f32.to_radians(),
        direction - side * 20.0_f32.to_radians(),
        direction - side * 45.0_f32.to_radians(),
        direction - side * 90.0_f32.to_radians(),
    ]
}

fn turn_limited_actor_intent(
    current: CharacterMoveIntent,
    face_direction: f32,
    desired: CharacterMoveIntent,
    delta: f32,
) -> CharacterMoveIntent {
    let CharacterMoveIntent::Moving {
        direction: desired_direction,
    } = desired
    else {
        return desired;
    };

    let current_direction = current.direction().unwrap_or(face_direction);
    let max_turn = ACTOR_TURN_SPEED.to_radians() * delta;
    let turn = angle_delta(desired_direction, current_direction).clamp(-max_turn, max_turn);
    CharacterMoveIntent::Moving {
        direction: current_direction + turn,
    }
}

fn angle_delta(a: f32, b: f32) -> f32 {
    (a - b + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

fn direction_toward(pos: &Position, target: &Position) -> f32 {
    let dx = target.x - pos.x;
    let dz = target.z - pos.z;
    dx.atan2(dz)
}

fn horizontal_distance_sq(a: &Position, b: &Position) -> f32 {
    let dx = a.x - b.x;
    let dz = a.z - b.z;
    dx.mul_add(dx, dz * dz)
}

fn apply_player_moves(query: &mut PlayerMovementQuery, planned_moves: &[PlannedCharacterMove]) {
    for planned_move in planned_moves {
        let Ok((_, mut pos, mut motion, _, _)) = query.get_mut(planned_move.entity) else {
            continue;
        };

        if overlapping_character(planned_move, planned_moves).is_some() {
            pos.y = planned_move.target.y;
        } else {
            *pos = planned_move.target;
        }
        motion.0 = planned_move.target_vertical_velocity;
    }
}

fn apply_actor_moves(
    delta: f32,
    players: &PlayerMap,
    actors: &mut ActorMap,
    query: &mut ActorMovementQuery,
    planned_moves: &[PlannedCharacterMove],
) {
    let mut rng = rng();
    for planned_move in planned_moves {
        let Ok((_, id, mut pos, mut motion, mut move_intent, mut face_dir)) = query.get_mut(planned_move.entity) else {
            continue;
        };
        let Some(info) = actors.0.get_mut(id) else {
            continue;
        };

        let overlapping_move = overlapping_character(planned_move, planned_moves);
        if overlapping_move.is_some() {
            pos.y = planned_move.target.y;
        } else {
            *pos = planned_move.target;
        }
        motion.0 = planned_move.target_vertical_velocity;

        if planned_move.blocked || overlapping_move.is_some() {
            let direction = if let Some(other) = overlapping_move {
                separation_direction(&planned_move.start, &other.start, &mut rng)
            } else {
                rng.random_range(0.0..std::f32::consts::TAU)
            };
            let desired_intent = CharacterMoveIntent::Moving { direction };
            let selected_intent = turn_limited_actor_intent(*move_intent, face_dir.0, desired_intent, delta);
            *move_intent = selected_intent;
            if let Some(direction) = selected_intent.direction() {
                face_dir.0 = direction;
            }
            info.avoidance_timer = ACTOR_AVOIDANCE_TIME;
            maybe_broadcast_actor_move_intent(players, *id, *pos, selected_intent, motion.0, info);
        }
    }
}

fn separation_direction(pos: &Position, other_pos: &Position, rng: &mut ThreadRng) -> f32 {
    let dx = pos.x - other_pos.x;
    let dz = pos.z - other_pos.z;
    if dx.hypot(dz) <= f32::EPSILON {
        rng.random_range(0.0..std::f32::consts::TAU)
    } else {
        dx.atan2(dz)
    }
}

fn random_avoidance_side(rng: &mut ThreadRng) -> f32 {
    if rng.random_bool(0.5) { 1.0 } else { -1.0 }
}

fn maybe_broadcast_actor_move_intent(
    players: &PlayerMap,
    id: ActorId,
    pos: Position,
    move_intent: CharacterMoveIntent,
    vertical_velocity: f32,
    info: &mut ActorInfo,
) {
    if !actor_move_intent_should_broadcast(
        info.last_broadcast_move_intent,
        move_intent,
        info.move_intent_send_timer,
    ) {
        return;
    }

    broadcast_actor_move_intent(players, id, pos, move_intent, vertical_velocity);
    info.last_broadcast_move_intent = move_intent;
    info.move_intent_send_timer = 0.0;
}

fn actor_move_intent_should_broadcast(
    last_broadcast: CharacterMoveIntent,
    current: CharacterMoveIntent,
    send_timer: f32,
) -> bool {
    let last_dir = last_broadcast.direction();
    let current_dir = current.direction();
    if last_dir.is_some() != current_dir.is_some() {
        return true;
    }

    match (current_dir, last_dir) {
        (Some(current), Some(last)) => {
            send_timer >= ACTOR_MOVE_INTENT_SEND_COOLDOWN
                && angle_delta(current, last).abs() >= ACTOR_MOVE_INTENT_DIR_CHANGE_THRESHOLD.to_radians()
        }
        _ => false,
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
