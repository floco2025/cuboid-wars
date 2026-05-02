use bevy::prelude::*;
use rand::{RngExt, rng, rngs::ThreadRng};

use super::network::broadcast_to_all;
use crate::{
    constants::{
        ACTOR_AVOIDANCE_TIME, ACTOR_DIRECTION_UPDATE_EPSILON, ACTOR_GO_TO_REACHED_DISTANCE,
        ACTOR_INTENT_CHANGE_COOLDOWN,
    },
    resources::{ActorInfo, ActorMap, PlayerInfo, PlayerMap},
};
use common::{
    constants::{ACTOR_SPEED, PHYSICS_EPSILON},
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

    plan_player_moves(delta, &collision_world, &players, &player_query, &mut planned_moves);
    plan_actor_moves(
        delta,
        &collision_world,
        &players,
        &mut actors,
        &actor_starts,
        &mut actor_query,
        &mut planned_moves,
    );
    apply_player_moves(&mut player_query, &planned_moves);
    apply_actor_moves(&players, &mut actor_query, &planned_moves);
}

fn plan_player_moves(
    delta: f32,
    collision_world: &CollisionWorld,
    players: &PlayerMap,
    query: &PlayerMovementQuery,
    planned_moves: &mut Vec<PlannedCharacterMove>,
) {
    for (entity, pos, motion, move_intent, player_id) in query.iter() {
        let is_stunned = players.0.get(player_id).is_some_and(|info| info.stun_timer > 0.0);
        let has_speed_power_up = players.0.get(player_id).is_some_and(PlayerInfo::has_speed);
        let velocity = move_intent.to_player_horizontal_velocity(has_speed_power_up);
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
            target_xz.x,
            target_xz.z,
            delta,
        );

        planned_moves.push(PlannedCharacterMove {
            entity,
            start: *pos,
            target: step.position,
            target_vertical_velocity: step.vertical_velocity,
            blocked: step.blocked,
        });
    }
}

fn plan_actor_moves(
    delta: f32,
    collision_world: &CollisionWorld,
    players: &PlayerMap,
    actors: &mut ActorMap,
    actor_starts: &[(Entity, Position)],
    query: &mut ActorMovementQuery,
    planned_moves: &mut Vec<PlannedCharacterMove>,
) {
    let mut rng = rng();
    for (entity, id, pos, motion, mut move_intent, mut face_dir) in query.iter_mut() {
        let Some(info) = actors.0.get_mut(id) else {
            continue;
        };
        let current_intent = *move_intent;
        let (selected_intent, step, used_avoidance) = if info.intent_change_cooldown > 0.0 {
            let step = step_actor_move(&pos, motion.0, current_intent, delta, collision_world);
            (current_intent, step, false)
        } else {
            let desired_intent = stable_actor_intent(
                current_intent,
                actor_desired_intent(info, &pos).unwrap_or(current_intent),
            );
            select_actor_move(
                entity,
                &pos,
                motion.0,
                desired_intent,
                info,
                delta,
                collision_world,
                planned_moves,
                actor_starts,
                &mut rng,
            )
        };

        if used_avoidance {
            info.avoidance_timer = ACTOR_AVOIDANCE_TIME;
        }
        if actor_intent_changed(current_intent, selected_intent) {
            *move_intent = selected_intent;
            if let Some(direction) = selected_intent.direction() {
                face_dir.0 = direction;
            }
            info.intent_change_cooldown = ACTOR_INTENT_CHANGE_COOLDOWN;
            broadcast_actor_move_intent(players, *id, *pos, selected_intent, motion.0);
        }

        planned_moves.push(PlannedCharacterMove {
            entity,
            start: *pos,
            target: step.position,
            target_vertical_velocity: step.vertical_velocity,
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
    desired_intent: CharacterMoveIntent,
    info: &mut ActorInfo,
    delta: f32,
    collision_world: &CollisionWorld,
    planned_moves: &[PlannedCharacterMove],
    actor_starts: &[(Entity, Position)],
    rng: &mut ThreadRng,
) -> (CharacterMoveIntent, common::physics::CharacterMovementResult, bool) {
    let Some(direction) = desired_intent.direction() else {
        let step = step_actor_move(pos, vertical_velocity, desired_intent, delta, collision_world);
        return (desired_intent, step, false);
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
        let step = step_actor_move(pos, vertical_velocity, candidate_intent, delta, collision_world);
        let planned_move = PlannedCharacterMove {
            entity,
            start: *pos,
            target: step.position,
            target_vertical_velocity: step.vertical_velocity,
            blocked: step.blocked,
        };
        let blocked = step.blocked || planned_move_overlaps_character(&planned_move, planned_moves, actor_starts);
        if index == 0 {
            fallback = Some((candidate_intent, step));
        }
        if !blocked {
            return (candidate_intent, step, index != 0);
        }
    }

    let (intent, step) = fallback.expect("steering directions always include direct movement");
    (intent, step, false)
}

fn step_actor_move(
    pos: &Position,
    vertical_velocity: f32,
    move_intent: CharacterMoveIntent,
    delta: f32,
    collision_world: &CollisionWorld,
) -> common::physics::CharacterMovementResult {
    let velocity = move_intent.to_horizontal_velocity(ACTOR_SPEED);
    let target_x = velocity.x.mul_add(delta, pos.x);
    let target_z = velocity.z.mul_add(delta, pos.z);
    step_character_movement(
        pos,
        vertical_velocity,
        collision_world,
        false,
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

fn stable_actor_intent(current: CharacterMoveIntent, desired: CharacterMoveIntent) -> CharacterMoveIntent {
    match (current, desired) {
        (CharacterMoveIntent::Moving { direction: current }, CharacterMoveIntent::Moving { direction: desired })
            if angle_delta(current, desired).abs() < ACTOR_DIRECTION_UPDATE_EPSILON =>
        {
            CharacterMoveIntent::Moving { direction: current }
        }
        _ => desired,
    }
}

fn actor_intent_changed(current: CharacterMoveIntent, next: CharacterMoveIntent) -> bool {
    match (current, next) {
        (CharacterMoveIntent::Idle, CharacterMoveIntent::Idle) => false,
        (CharacterMoveIntent::Moving { direction: current }, CharacterMoveIntent::Moving { direction: next }) => {
            angle_delta(current, next).abs() >= ACTOR_DIRECTION_UPDATE_EPSILON
        }
        _ => true,
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

fn apply_actor_moves(players: &PlayerMap, query: &mut ActorMovementQuery, planned_moves: &[PlannedCharacterMove]) {
    let mut rng = rng();
    for planned_move in planned_moves {
        let Ok((_, id, mut pos, mut motion, mut move_intent, mut face_dir)) = query.get_mut(planned_move.entity) else {
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
            *move_intent = CharacterMoveIntent::Moving { direction };
            face_dir.0 = direction;
            broadcast_actor_move_intent(players, *id, *pos, *move_intent, motion.0);
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
