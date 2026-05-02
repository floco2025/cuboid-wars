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

// Actor behavior decides what an actor wants: patrol or a remembered go-to position.
// This module turns that behavior target into a concrete movement intent for this
// frame, including committed wall avoidance and yielding to other characters.

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
    let actor_order = sorted_actor_plan_order(query, actors);

    for ActorPlanOrder { entity, .. } in actor_order {
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

#[derive(Copy, Clone, Debug, PartialEq)]
struct ActorPlanOrder {
    entity: Entity,
    target_distance_sq: f32,
    id: ActorId,
}

fn sorted_actor_plan_order(query: &ActorMovementQuery, actors: &ActorMap) -> Vec<ActorPlanOrder> {
    let mut order: Vec<ActorPlanOrder> = query
        .iter()
        .map(|(entity, id, pos, _, _, _)| ActorPlanOrder {
            entity,
            target_distance_sq: actor_target_distance_sq(pos, actors.0.get(id)),
            id: *id,
        })
        .collect();
    sort_actor_plan_order(&mut order);
    order
}

fn sort_actor_plan_order(order: &mut [ActorPlanOrder]) {
    order.sort_by(|a, b| {
        a.target_distance_sq
            .total_cmp(&b.target_distance_sq)
            .then_with(|| a.id.0.cmp(&b.id.0))
    });
}

fn actor_target_distance_sq(pos: &Position, info: Option<&ActorInfo>) -> f32 {
    info.and_then(|info| info.go_to_position)
        .map_or(f32::INFINITY, |target| horizontal_distance_sq(pos, &target))
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
        info.wall_avoidance_direction = None;
        return idle_actor_move(
            pos,
            vertical_velocity,
            actor_speed,
            actor_physics,
            delta,
            collision_world,
        );
    };

    if let Some((intent, step)) = continue_wall_avoidance_if_needed(
        entity,
        pos,
        vertical_velocity,
        direction,
        go_to_position,
        info,
        actor_speed,
        actor_physics,
        delta,
        collision_world,
        planned_moves,
        actor_starts,
    ) {
        return (intent, step);
    }

    match try_normal_steering_candidates(
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
        CandidateSearch::Accepted {
            intent,
            step,
            wall_avoidance_direction,
        } => {
            info.wall_avoidance_direction = wall_avoidance_direction;
            return (intent, step);
        }
        CandidateSearch::BlockedByCharacter => {
            return idle_actor_move(
                pos,
                vertical_velocity,
                actor_speed,
                actor_physics,
                delta,
                collision_world,
            );
        }
        CandidateSearch::BlockedByWorld => {}
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

#[expect(
    clippy::too_many_arguments,
    reason = "Candidate testing needs the same movement context as normal actor planning"
)]
fn continue_wall_avoidance_if_needed(
    entity: Entity,
    pos: &Position,
    vertical_velocity: f32,
    direction: f32,
    go_to_position: Option<Position>,
    info: &mut ActorInfo,
    actor_speed: f32,
    actor_physics: CharacterPhysicsConfig,
    delta: f32,
    collision_world: &CollisionWorld,
    planned_moves: &[CharacterMovePlan],
    actor_starts: &[(Entity, Position)],
) -> Option<(CharacterMoveIntent, common::physics::CharacterMovementResult)> {
    let avoidance_direction = info.wall_avoidance_direction?;
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
        return None;
    }

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
        CandidateMove::Accepted { intent, step } => Some((intent, step)),
        CandidateMove::BlockedByCharacter => Some(idle_actor_move(
            pos,
            vertical_velocity,
            actor_speed,
            actor_physics,
            delta,
            collision_world,
        )),
        CandidateMove::BlockedByWorld { .. } => {
            let next_move = try_opposite_wall_avoidance_direction(
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
            );
            if let Some((intent, _)) = &next_move {
                info.wall_avoidance_direction = intent.direction();
            }
            next_move
        }
    }
}

enum CandidateSearch {
    Accepted {
        intent: CharacterMoveIntent,
        step: common::physics::CharacterMovementResult,
        wall_avoidance_direction: Option<f32>,
    },
    BlockedByCharacter,
    BlockedByWorld,
}

#[expect(
    clippy::too_many_arguments,
    reason = "Candidate testing needs the same movement context as normal actor planning"
)]
fn try_normal_steering_candidates(
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
) -> CandidateSearch {
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
                return CandidateSearch::Accepted {
                    intent,
                    step,
                    wall_avoidance_direction: (index != 0).then_some(candidate_direction),
                };
            }
            CandidateMove::BlockedByCharacter => {
                was_blocked_by_character = true;
            }
            CandidateMove::BlockedByWorld { .. } => {}
        }
    }

    if was_blocked_by_character {
        CandidateSearch::BlockedByCharacter
    } else {
        CandidateSearch::BlockedByWorld
    }
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
        wall_avoidance_directions(direction, side),
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
        [opposite_wall_avoidance_direction(current_direction)],
        actor_speed,
        actor_physics,
        delta,
        collision_world,
        planned_moves,
        actor_starts,
    )
}

fn wall_avoidance_directions(direction: f32, side: f32) -> [f32; 2] {
    [
        direction + side * std::f32::consts::FRAC_PI_2,
        direction - side * std::f32::consts::FRAC_PI_2,
    ]
}

fn opposite_wall_avoidance_direction(current_direction: f32) -> f32 {
    current_direction + std::f32::consts::PI
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
    // Wall avoidance is allowed to keep a blocked side-contact result only
    // when Rapier still moved the actor meaningfully. That lets actors slide
    // around geometry edges without accepting contact jitter as progress.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_near(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 0.0001,
            "expected {actual} to be near {expected}"
        );
    }

    fn order(entity_bits: u64, target_distance_sq: f32, id: u32) -> ActorPlanOrder {
        ActorPlanOrder {
            entity: Entity::from_bits(entity_bits),
            target_distance_sq,
            id: ActorId(id),
        }
    }

    #[test]
    fn actor_plan_order_prioritizes_closer_go_to_target() {
        let mut order = vec![order(1, 9.0, 1), order(2, 1.0, 2), order(3, 4.0, 3)];

        sort_actor_plan_order(&mut order);

        assert_eq!(
            order.iter().map(|entry| entry.id).collect::<Vec<_>>(),
            vec![ActorId(2), ActorId(3), ActorId(1),]
        );
    }

    #[test]
    fn actor_plan_order_uses_actor_id_as_tie_breaker() {
        let mut order = vec![order(1, 4.0, 3), order(2, 4.0, 1), order(3, 4.0, 2)];

        sort_actor_plan_order(&mut order);

        assert_eq!(
            order.iter().map(|entry| entry.id).collect::<Vec<_>>(),
            vec![ActorId(1), ActorId(2), ActorId(3),]
        );
    }

    #[test]
    fn actor_without_go_to_position_plans_after_targeted_actor() {
        let pos = Position::default();
        let targeted = ActorInfo {
            entity: Entity::from_bits(1),
            kind: common::protocol::ActorKind::Automaton,
            direction_timer: 0.0,
            patrol_intent: CharacterMoveIntent::Idle,
            go_to_position: Some(Position { x: 1.0, y: 0.0, z: 0.0 }),
            wall_avoidance_direction: None,
            last_broadcast_move_intent: CharacterMoveIntent::Idle,
            move_intent_send_timer: 0.0,
        };

        assert!(actor_target_distance_sq(&pos, Some(&targeted)).is_finite());
        assert_eq!(actor_target_distance_sq(&pos, None), f32::INFINITY);
    }

    #[test]
    fn wall_avoidance_directions_try_one_side_then_the_other() {
        let directions = wall_avoidance_directions(1.0, 1.0);

        assert_near(directions[0], 1.0 + std::f32::consts::FRAC_PI_2);
        assert_near(directions[1], 1.0 - std::f32::consts::FRAC_PI_2);
    }

    #[test]
    fn wall_avoidance_directions_respect_randomized_side_order() {
        let directions = wall_avoidance_directions(1.0, -1.0);

        assert_near(directions[0], 1.0 - std::f32::consts::FRAC_PI_2);
        assert_near(directions[1], 1.0 + std::f32::consts::FRAC_PI_2);
    }

    #[test]
    fn opposite_wall_avoidance_direction_turns_around() {
        assert_near(opposite_wall_avoidance_direction(1.0), 1.0 + std::f32::consts::PI);
    }

    #[test]
    fn blocked_step_needs_meaningful_progress() {
        let start = Position::default();
        let too_short = Position {
            x: 0.24,
            y: 0.0,
            z: 0.0,
        };
        let useful = Position {
            x: 0.26,
            y: 0.0,
            z: 0.0,
        };

        assert!(!blocked_step_made_useful_progress(&start, &too_short, 5.0, 0.1));
        assert!(blocked_step_made_useful_progress(&start, &useful, 5.0, 0.1));
    }
}
