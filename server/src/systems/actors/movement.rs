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
        CharacterMovePlan, CharacterMovementResult, CharacterVerticalVelocity, CollisionWorld,
        blocking_character_move_plan, character_move_plan_is_blocked, step_character_movement,
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
        let move_context = ActorMoveContext {
            entity,
            pos: &pos,
            vertical_velocity: motion.0,
            actor_speed: actor_config.speed,
            actor_physics,
            delta,
            collision_world,
            planned_moves,
            actor_starts,
        };
        let selected_move = select_actor_move(&move_context, desired_intent, info.go_to_position, info, &mut rng);

        *move_intent = selected_move.intent;
        if let Some(direction) = selected_move.intent.direction() {
            face_dir.0 = direction;
        }
        maybe_broadcast_actor_move_intent(players, *id, *pos, selected_move.intent, motion.0, info);

        planned_moves.push(CharacterMovePlan {
            entity,
            start: *pos,
            target: selected_move.step.position,
            target_vertical_velocity: selected_move.step.vertical_velocity,
            physics: actor_physics,
            blocked: selected_move.step.blocked,
        });
    }
}

#[derive(Copy, Clone)]
struct SelectedActorMove {
    intent: CharacterMoveIntent,
    step: CharacterMovementResult,
}

struct ActorMoveContext<'a> {
    entity: Entity,
    pos: &'a Position,
    vertical_velocity: f32,
    actor_speed: f32,
    actor_physics: CharacterPhysicsConfig,
    delta: f32,
    collision_world: &'a CollisionWorld,
    planned_moves: &'a [CharacterMovePlan],
    actor_starts: &'a [(Entity, Position)],
}

impl ActorMoveContext<'_> {
    fn idle_move(&self) -> SelectedActorMove {
        let intent = CharacterMoveIntent::Idle;
        SelectedActorMove {
            intent,
            step: self.step_actor_move(intent, self.delta),
        }
    }

    fn step_actor_move(&self, move_intent: CharacterMoveIntent, delta: f32) -> CharacterMovementResult {
        let velocity = move_intent.to_horizontal_velocity(self.actor_speed);
        let target_x = velocity.x.mul_add(delta, self.pos.x);
        let target_z = velocity.z.mul_add(delta, self.pos.z);
        step_character_movement(
            self.pos,
            self.vertical_velocity,
            self.collision_world,
            false,
            self.actor_physics,
            target_x,
            target_z,
            delta,
        )
    }

    fn evaluate_candidate(&self, intent: CharacterMoveIntent) -> MoveCandidateResult {
        let selected = SelectedActorMove {
            intent,
            step: self.step_actor_move(intent, self.delta),
        };
        let planned_move = CharacterMovePlan {
            entity: self.entity,
            start: *self.pos,
            target: selected.step.position,
            target_vertical_velocity: selected.step.vertical_velocity,
            physics: self.actor_physics,
            blocked: selected.step.blocked,
        };

        if character_move_plan_is_blocked(&planned_move, self.planned_moves, self.actor_starts) {
            return MoveCandidateResult::BlockedByCharacter;
        }
        if selected.step.blocked {
            return MoveCandidateResult::BlockedByWorld { selected };
        }

        MoveCandidateResult::Accepted { selected }
    }
}

pub(crate) fn apply_actor_moves(query: &mut ActorMovementQuery, planned_moves: &[CharacterMovePlan]) {
    for planned_move in planned_moves {
        let Ok((_, _, mut pos, mut motion, _, _)) = query.get_mut(planned_move.entity) else {
            continue;
        };

        let overlapping_move = blocking_character_move_plan(planned_move, planned_moves);
        if overlapping_move.is_some() {
            pos.y = planned_move.target.y;
        } else {
            *pos = planned_move.target;
        }
        motion.0 = planned_move.target_vertical_velocity;
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

fn select_actor_move(
    context: &ActorMoveContext,
    desired_intent: CharacterMoveIntent,
    go_to_position: Option<Position>,
    info: &mut ActorInfo,
    rng: &mut ThreadRng,
) -> SelectedActorMove {
    let Some(direction) = desired_intent.direction() else {
        info.wall_avoidance_direction = None;
        return context.idle_move();
    };

    if let Some(selected_move) = continue_wall_avoidance_if_needed(context, direction, go_to_position, info) {
        return selected_move;
    }

    match choose_steering_move(context, direction, rng) {
        SteeringMoveChoice::Accepted {
            selected,
            wall_avoidance_direction,
        } => {
            info.wall_avoidance_direction = wall_avoidance_direction;
            return selected;
        }
        SteeringMoveChoice::BlockedByCharacter => {
            return context.idle_move();
        }
        SteeringMoveChoice::BlockedByWorld => {}
    }

    if let Some(selected_move) = choose_new_wall_avoidance_move(context, direction, rng) {
        info.wall_avoidance_direction = selected_move.intent.direction();
        return selected_move;
    }

    context.idle_move()
}

fn continue_wall_avoidance_if_needed(
    context: &ActorMoveContext,
    direction: f32,
    go_to_position: Option<Position>,
    info: &mut ActorInfo,
) -> Option<SelectedActorMove> {
    let avoidance_direction = info.wall_avoidance_direction?;
    if direct_path_is_clear_enough(context, direction, go_to_position) {
        info.wall_avoidance_direction = None;
        return None;
    }

    let avoidance_intent = CharacterMoveIntent::Moving {
        direction: avoidance_direction,
    };
    match context.evaluate_candidate(avoidance_intent) {
        MoveCandidateResult::Accepted { selected } => Some(selected),
        MoveCandidateResult::BlockedByCharacter => Some(context.idle_move()),
        MoveCandidateResult::BlockedByWorld { .. } => {
            let next_move = choose_opposite_wall_avoidance_move(context, avoidance_direction);
            if let Some(selected_move) = &next_move {
                info.wall_avoidance_direction = selected_move.intent.direction();
            }
            next_move
        }
    }
}

// Static-world blocking and character blocking deliberately lead to different
// behavior: static walls can start wall avoidance, while other characters make
// this actor yield for the frame.
enum SteeringMoveChoice {
    Accepted {
        selected: SelectedActorMove,
        wall_avoidance_direction: Option<f32>,
    },
    BlockedByCharacter,
    BlockedByWorld,
}

fn choose_steering_move(context: &ActorMoveContext, direction: f32, rng: &mut ThreadRng) -> SteeringMoveChoice {
    let mut was_blocked_by_character = false;
    let avoidance_side = random_avoidance_side(rng);
    for (index, candidate_direction) in steering_directions(direction, avoidance_side).into_iter().enumerate() {
        let candidate_intent = CharacterMoveIntent::Moving {
            direction: candidate_direction,
        };
        match context.evaluate_candidate(candidate_intent) {
            MoveCandidateResult::Accepted { selected } => {
                return SteeringMoveChoice::Accepted {
                    selected,
                    wall_avoidance_direction: (index != 0).then_some(candidate_direction),
                };
            }
            MoveCandidateResult::BlockedByCharacter => {
                was_blocked_by_character = true;
            }
            MoveCandidateResult::BlockedByWorld { .. } => {}
        }
    }

    if was_blocked_by_character {
        SteeringMoveChoice::BlockedByCharacter
    } else {
        SteeringMoveChoice::BlockedByWorld
    }
}

enum MoveCandidateResult {
    Accepted { selected: SelectedActorMove },
    BlockedByCharacter,
    BlockedByWorld { selected: SelectedActorMove },
}

fn choose_new_wall_avoidance_move(
    context: &ActorMoveContext,
    direction: f32,
    rng: &mut ThreadRng,
) -> Option<SelectedActorMove> {
    let side = random_avoidance_side(rng);
    choose_wall_avoidance_move(context, wall_avoidance_directions(direction, side))
}

fn choose_opposite_wall_avoidance_move(
    context: &ActorMoveContext,
    current_direction: f32,
) -> Option<SelectedActorMove> {
    choose_wall_avoidance_move(context, [opposite_wall_avoidance_direction(current_direction)])
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

fn choose_wall_avoidance_move<const N: usize>(
    context: &ActorMoveContext,
    directions: [f32; N],
) -> Option<SelectedActorMove> {
    // Wall avoidance is allowed to keep a blocked side-contact result only
    // when Rapier still moved the actor meaningfully. That lets actors slide
    // around geometry edges without accepting contact jitter as progress.
    for direction in directions {
        let intent = CharacterMoveIntent::Moving { direction };
        match context.evaluate_candidate(intent) {
            MoveCandidateResult::Accepted { selected } => return Some(selected),
            MoveCandidateResult::BlockedByWorld { selected }
                if blocked_step_made_useful_progress(
                    context.pos,
                    &selected.step.position,
                    context.actor_speed,
                    context.delta,
                ) =>
            {
                return Some(selected);
            }
            MoveCandidateResult::BlockedByCharacter | MoveCandidateResult::BlockedByWorld { .. } => {}
        }
    }
    None
}

fn blocked_step_made_useful_progress(start: &Position, target: &Position, actor_speed: f32, delta: f32) -> bool {
    let requested_distance = actor_speed * delta;
    let useful_distance = requested_distance * 0.5;
    horizontal_distance_sq(start, target) > useful_distance * useful_distance
}

fn direct_path_is_clear_enough(context: &ActorMoveContext, direction: f32, go_to_position: Option<Position>) -> bool {
    let direct_intent = CharacterMoveIntent::Moving { direction };
    let step = context.step_actor_move(direct_intent, ACTOR_DIRECT_PATH_PROBE_TIME);
    if step.blocked {
        return false;
    }

    go_to_position.is_none_or(|target| {
        horizontal_distance_sq(&step.position, &target) < horizontal_distance_sq(context.pos, &target)
    })
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
