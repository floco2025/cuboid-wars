use bevy::prelude::*;
use rand::{RngExt, rng, rngs::ThreadRng};

use crate::{
    constants::{ACTOR_AVOIDANCE_TIME, ACTOR_GO_TO_REACHED_DISTANCE},
    resources::{ActorInfo, ActorMap, PlayerInfo, PlayerMap},
    systems::actors::{
        maybe_broadcast_actor_move_intent,
        steering::{actor_desired_intent, random_avoidance_side, separation_direction, steering_directions},
    },
};
use common::{
    config::{CharacterPhysicsConfig, GameplayConfig},
    constants::PHYSICS_EPSILON,
    markers::{ActorMarker, PlayerMarker},
    physics::{
        CharacterMovePlan, CharacterVerticalMotion, CollisionWorld, blocking_character_move_plan,
        character_move_plan_is_blocked, overlapping_character, step_character_movement,
    },
    protocol::{ActorId, CharacterMoveIntent, FaceDirection, PlayerId, Position},
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
    apply_actor_moves(&players, &mut actors, &mut actor_query, &planned_moves);
}

fn plan_player_moves(
    delta: f32,
    collision_world: &CollisionWorld,
    gameplay_config: &GameplayConfig,
    players: &PlayerMap,
    query: &PlayerMovementQuery,
    planned_moves: &mut Vec<CharacterMovePlan>,
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

        planned_moves.push(CharacterMovePlan {
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
    planned_moves: &mut Vec<CharacterMovePlan>,
) {
    let actor_config = &gameplay_config.characters.actor;
    let actor_physics = actor_config.physics();
    let mut rng = rng();
    for (entity, id, pos, motion, mut move_intent, mut face_dir) in query.iter_mut() {
        let Some(info) = actors.0.get_mut(id) else {
            continue;
        };
        info.move_intent_send_timer += delta;
        let desired_intent = actor_desired_intent(&mut info.go_to_position, &pos, ACTOR_GO_TO_REACHED_DISTANCE)
            .unwrap_or(info.patrol_intent);
        let (selected_intent, step, used_avoidance) = select_actor_move(
            entity,
            &pos,
            motion.0,
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

        planned_moves.push(CharacterMovePlan {
            entity,
            start: *pos,
            target: step.position,
            target_vertical_velocity: step.vertical_velocity,
            physics: actor_physics,
            blocked: step.blocked,
        });
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Candidate testing needs the same movement context as normal actor planning"
)]
fn select_actor_move(
    entity: Entity,
    pos: &Position,
    vertical_velocity: f32,
    desired_intent: CharacterMoveIntent,
    info: &mut ActorInfo,
    actor_speed: f32,
    actor_physics: CharacterPhysicsConfig,
    delta: f32,
    collision_world: &CollisionWorld,
    planned_moves: &[CharacterMovePlan],
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

    for (index, candidate_direction) in steering_directions(direction, info.avoidance_side)
        .into_iter()
        .enumerate()
    {
        let candidate_intent = CharacterMoveIntent::Moving {
            direction: candidate_direction,
        };
        let step = step_actor_move(
            pos,
            vertical_velocity,
            candidate_intent,
            actor_speed,
            actor_physics,
            delta,
            collision_world,
        );
        let planned_move = CharacterMovePlan {
            entity,
            start: *pos,
            target: step.position,
            target_vertical_velocity: step.vertical_velocity,
            physics: actor_physics,
            blocked: step.blocked,
        };
        let blocked = step.blocked || character_move_plan_is_blocked(&planned_move, planned_moves, actor_starts);
        if !blocked {
            return (candidate_intent, step, index != 0);
        }
    }

    if let Some(actor_pos) = nearest_actor_position(pos, entity, actor_starts) {
        let selected_intent = CharacterMoveIntent::Moving {
            direction: separation_direction(pos, actor_pos, rng),
        };
        let step = step_actor_move(
            pos,
            vertical_velocity,
            selected_intent,
            actor_speed,
            actor_physics,
            delta,
            collision_world,
        );
        let planned_move = CharacterMovePlan {
            entity,
            start: *pos,
            target: step.position,
            target_vertical_velocity: step.vertical_velocity,
            physics: actor_physics,
            blocked: step.blocked,
        };
        let blocked = step.blocked || character_move_plan_is_blocked(&planned_move, planned_moves, actor_starts);
        if !blocked {
            return (selected_intent, step, true);
        }
    }

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
    (selected_intent, step, true)
}

fn nearest_actor_position<'a>(
    pos: &Position,
    entity: Entity,
    actor_starts: &'a [(Entity, Position)],
) -> Option<&'a Position> {
    actor_starts
        .iter()
        .filter(|(other_entity, _)| *other_entity != entity)
        .min_by(|(_, a), (_, b)| horizontal_distance_sq(pos, a).total_cmp(&horizontal_distance_sq(pos, b)))
        .map(|(_, actor_pos)| actor_pos)
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

fn horizontal_distance_sq(a: &Position, b: &Position) -> f32 {
    let dx = a.x - b.x;
    let dz = a.z - b.z;
    dx.mul_add(dx, dz * dz)
}

fn apply_player_moves(query: &mut PlayerMovementQuery, planned_moves: &[CharacterMovePlan]) {
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
    players: &PlayerMap,
    actors: &mut ActorMap,
    query: &mut ActorMovementQuery,
    planned_moves: &[CharacterMovePlan],
) {
    let mut rng = rng();
    for planned_move in planned_moves {
        let Ok((_, id, mut pos, mut motion, mut move_intent, mut face_dir)) = query.get_mut(planned_move.entity) else {
            continue;
        };
        let Some(info) = actors.0.get_mut(id) else {
            continue;
        };

        let overlapping_move = blocking_character_move_plan(planned_move, planned_moves);
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
            *move_intent = desired_intent;
            if let Some(direction) = desired_intent.direction() {
                face_dir.0 = direction;
            }
            if planned_move.blocked && overlapping_move.is_none() {
                info.go_to_position = None;
                info.patrol_intent = desired_intent;
            }
            info.avoidance_timer = ACTOR_AVOIDANCE_TIME;
            maybe_broadcast_actor_move_intent(players, *id, *pos, desired_intent, motion.0, info);
        }
    }
}
