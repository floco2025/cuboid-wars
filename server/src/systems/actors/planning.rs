use bevy::prelude::*;
use rand::{rng, rngs::ThreadRng};

use super::{
    network::maybe_broadcast_actor_move_intent,
    steering::{actor_desired_intent, random_avoidance_side, steering_directions},
};
use crate::{
    constants::{ACTOR_DIRECT_PATH_PROBE_TIME, ACTOR_GO_TO_REACHED_DISTANCE},
    resources::{ActorInfo, ActorMap, PlayerMap},
};
use common::{
    config::{CharacterPhysicsConfig, GameplayConfig},
    markers::{ActorMarker, PlayerMarker},
    physics::{
        CharacterMovePlan, CharacterVerticalVelocity, CollisionWorld, character_move_plan_is_blocked,
        step_character_movement,
    },
    protocol::{ActorId, CharacterMoveIntent, FaceDirection, Position},
};

pub(crate) type ActorMovementQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ActorId,
        &'static mut Position,
        &'static mut CharacterVerticalVelocity,
        &'static mut CharacterMoveIntent,
        &'static mut FaceDirection,
    ),
    (With<ActorMarker>, Without<PlayerMarker>),
>;

pub(crate) fn plan_actor_moves(
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
    let mut actor_order: Vec<(Entity, f32, ActorId)> = query
        .iter()
        .map(|(entity, id, pos, _, _, _)| {
            let target_distance = actors
                .0
                .get(id)
                .and_then(|info| info.go_to_position)
                .map_or(f32::INFINITY, |target| horizontal_distance_sq(pos, &target));
            (entity, target_distance, *id)
        })
        .collect();
    actor_order.sort_by(|(_, a_distance, a_id), (_, b_distance, b_id)| {
        a_distance.total_cmp(b_distance).then_with(|| a_id.0.cmp(&b_id.0))
    });

    for (entity, _, _) in actor_order {
        let Ok((_, id, pos, motion, mut move_intent, mut face_dir)) = query.get_mut(entity) else {
            continue;
        };
        let Some(info) = actors.0.get_mut(id) else {
            continue;
        };
        info.move_intent_send_timer += delta;
        let desired_intent = actor_desired_intent(&mut info.go_to_position, &pos, ACTOR_GO_TO_REACHED_DISTANCE)
            .unwrap_or_else(|| {
                info.wall_avoidance_direction = None;
                info.patrol_intent
            });
        let (selected_intent, step) = select_actor_move(
            entity,
            &pos,
            motion.0,
            desired_intent,
            info.go_to_position,
            info,
            actor_config.speed,
            actor_physics,
            delta,
            collision_world,
            planned_moves,
            actor_starts,
            &mut rng,
        );

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
    go_to_position: Option<Position>,
    info: &mut ActorInfo,
    actor_speed: f32,
    actor_physics: CharacterPhysicsConfig,
    delta: f32,
    collision_world: &CollisionWorld,
    planned_moves: &[CharacterMovePlan],
    actor_starts: &[(Entity, Position)],
    rng: &mut ThreadRng,
) -> (CharacterMoveIntent, common::physics::CharacterMovementResult) {
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
        info.wall_avoidance_direction = None;
        return (selected_intent, step);
    };

    if let Some(avoidance_direction) = info.wall_avoidance_direction {
        if direct_path_is_clear_enough(
            pos,
            vertical_velocity,
            direction,
            go_to_position,
            actor_speed,
            actor_physics,
            collision_world,
        ) {
            info.wall_avoidance_direction = None;
        } else {
            let avoidance_intent = CharacterMoveIntent::Moving {
                direction: avoidance_direction,
            };
            match try_actor_candidate_move(
                entity,
                pos,
                vertical_velocity,
                avoidance_intent,
                actor_speed,
                actor_physics,
                delta,
                collision_world,
                planned_moves,
                actor_starts,
            ) {
                CandidateMove::Accepted { intent, step } => return (intent, step),
                CandidateMove::BlockedByCharacter => {
                    return idle_actor_move(
                        pos,
                        vertical_velocity,
                        actor_speed,
                        actor_physics,
                        delta,
                        collision_world,
                    );
                }
                CandidateMove::BlockedByWorld { .. } => {
                    if let Some((intent, step)) = try_opposite_wall_avoidance_direction(
                        entity,
                        pos,
                        vertical_velocity,
                        avoidance_direction,
                        actor_speed,
                        actor_physics,
                        delta,
                        collision_world,
                        planned_moves,
                        actor_starts,
                    ) {
                        info.wall_avoidance_direction = intent.direction();
                        return (intent, step);
                    }
                }
            }
        }
    }

    let mut was_blocked_by_character = false;
    let avoidance_side = random_avoidance_side(rng);
    for (index, candidate_direction) in steering_directions(direction, avoidance_side).into_iter().enumerate() {
        let candidate_intent = CharacterMoveIntent::Moving {
            direction: candidate_direction,
        };
        match try_actor_candidate_move(
            entity,
            pos,
            vertical_velocity,
            candidate_intent,
            actor_speed,
            actor_physics,
            delta,
            collision_world,
            planned_moves,
            actor_starts,
        ) {
            CandidateMove::Accepted { intent, step } => {
                if index != 0 {
                    info.wall_avoidance_direction = Some(candidate_direction);
                }
                return (intent, step);
            }
            CandidateMove::BlockedByCharacter => {
                was_blocked_by_character = true;
            }
            CandidateMove::BlockedByWorld { .. } => {}
        }
    }

    if was_blocked_by_character {
        return idle_actor_move(
            pos,
            vertical_velocity,
            actor_speed,
            actor_physics,
            delta,
            collision_world,
        );
    }

    if let Some((intent, step)) = try_new_wall_avoidance_direction(
        entity,
        pos,
        vertical_velocity,
        direction,
        actor_speed,
        actor_physics,
        delta,
        collision_world,
        planned_moves,
        actor_starts,
        rng,
    ) {
        info.wall_avoidance_direction = intent.direction();
        return (intent, step);
    }

    idle_actor_move(
        pos,
        vertical_velocity,
        actor_speed,
        actor_physics,
        delta,
        collision_world,
    )
}

enum CandidateMove {
    Accepted {
        intent: CharacterMoveIntent,
        step: common::physics::CharacterMovementResult,
    },
    BlockedByCharacter,
    BlockedByWorld {
        intent: CharacterMoveIntent,
        step: common::physics::CharacterMovementResult,
    },
}

#[expect(
    clippy::too_many_arguments,
    reason = "Candidate testing needs the same movement context as normal actor planning"
)]
fn try_actor_candidate_move(
    entity: Entity,
    pos: &Position,
    vertical_velocity: f32,
    intent: CharacterMoveIntent,
    actor_speed: f32,
    actor_physics: CharacterPhysicsConfig,
    delta: f32,
    collision_world: &CollisionWorld,
    planned_moves: &[CharacterMovePlan],
    actor_starts: &[(Entity, Position)],
) -> CandidateMove {
    let step = step_actor_move(
        pos,
        vertical_velocity,
        intent,
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

    if character_move_plan_is_blocked(&planned_move, planned_moves, actor_starts) {
        return CandidateMove::BlockedByCharacter;
    }
    if step.blocked {
        return CandidateMove::BlockedByWorld { intent, step };
    }

    CandidateMove::Accepted { intent, step }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Candidate testing needs the same movement context as normal actor planning"
)]
fn try_new_wall_avoidance_direction(
    entity: Entity,
    pos: &Position,
    vertical_velocity: f32,
    direction: f32,
    actor_speed: f32,
    actor_physics: CharacterPhysicsConfig,
    delta: f32,
    collision_world: &CollisionWorld,
    planned_moves: &[CharacterMovePlan],
    actor_starts: &[(Entity, Position)],
    rng: &mut ThreadRng,
) -> Option<(CharacterMoveIntent, common::physics::CharacterMovementResult)> {
    let side = random_avoidance_side(rng);
    try_wall_avoidance_directions(
        entity,
        pos,
        vertical_velocity,
        [
            direction + side * std::f32::consts::FRAC_PI_2,
            direction - side * std::f32::consts::FRAC_PI_2,
        ],
        actor_speed,
        actor_physics,
        delta,
        collision_world,
        planned_moves,
        actor_starts,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "Candidate testing needs the same movement context as normal actor planning"
)]
fn try_opposite_wall_avoidance_direction(
    entity: Entity,
    pos: &Position,
    vertical_velocity: f32,
    current_direction: f32,
    actor_speed: f32,
    actor_physics: CharacterPhysicsConfig,
    delta: f32,
    collision_world: &CollisionWorld,
    planned_moves: &[CharacterMovePlan],
    actor_starts: &[(Entity, Position)],
) -> Option<(CharacterMoveIntent, common::physics::CharacterMovementResult)> {
    try_wall_avoidance_directions(
        entity,
        pos,
        vertical_velocity,
        [current_direction + std::f32::consts::PI],
        actor_speed,
        actor_physics,
        delta,
        collision_world,
        planned_moves,
        actor_starts,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "Candidate testing needs the same movement context as normal actor planning"
)]
fn try_wall_avoidance_directions<const N: usize>(
    entity: Entity,
    pos: &Position,
    vertical_velocity: f32,
    directions: [f32; N],
    actor_speed: f32,
    actor_physics: CharacterPhysicsConfig,
    delta: f32,
    collision_world: &CollisionWorld,
    planned_moves: &[CharacterMovePlan],
    actor_starts: &[(Entity, Position)],
) -> Option<(CharacterMoveIntent, common::physics::CharacterMovementResult)> {
    for direction in directions {
        let intent = CharacterMoveIntent::Moving { direction };
        match try_actor_candidate_move(
            entity,
            pos,
            vertical_velocity,
            intent,
            actor_speed,
            actor_physics,
            delta,
            collision_world,
            planned_moves,
            actor_starts,
        ) {
            CandidateMove::Accepted { intent, step } => return Some((intent, step)),
            CandidateMove::BlockedByWorld { intent, step }
                if blocked_step_made_useful_progress(pos, &step.position, actor_speed, delta) =>
            {
                return Some((intent, step));
            }
            CandidateMove::BlockedByCharacter | CandidateMove::BlockedByWorld { .. } => {}
        }
    }
    None
}

fn blocked_step_made_useful_progress(start: &Position, target: &Position, actor_speed: f32, delta: f32) -> bool {
    let requested_distance = actor_speed * delta;
    let useful_distance = requested_distance * 0.5;
    horizontal_distance_sq(start, target) > useful_distance * useful_distance
}

fn idle_actor_move(
    pos: &Position,
    vertical_velocity: f32,
    actor_speed: f32,
    actor_physics: CharacterPhysicsConfig,
    delta: f32,
    collision_world: &CollisionWorld,
) -> (CharacterMoveIntent, common::physics::CharacterMovementResult) {
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
    (selected_intent, step)
}

fn direct_path_is_clear_enough(
    pos: &Position,
    vertical_velocity: f32,
    direction: f32,
    go_to_position: Option<Position>,
    actor_speed: f32,
    actor_physics: CharacterPhysicsConfig,
    collision_world: &CollisionWorld,
) -> bool {
    let direct_intent = CharacterMoveIntent::Moving { direction };
    let step = step_actor_move(
        pos,
        vertical_velocity,
        direct_intent,
        actor_speed,
        actor_physics,
        ACTOR_DIRECT_PATH_PROBE_TIME,
        collision_world,
    );
    if step.blocked {
        return false;
    }

    go_to_position
        .is_none_or(|target| horizontal_distance_sq(&step.position, &target) < horizontal_distance_sq(pos, &target))
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
