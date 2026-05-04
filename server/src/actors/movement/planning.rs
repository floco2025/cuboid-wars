use rand::{rng, rngs::ThreadRng};

use crate::{
    actors::{
        behavior::random_patrol_move_intent,
        network::maybe_broadcast_actor_move_intent,
        steering::{actor_desired_intent, random_avoidance_side, steering_directions},
    },
    config::ServerGameplayConfig,
    resources::{ActorInfo, ActorMap, PlayerMap},
};
use common::{
    config::GameplayConfig,
    physics::{CharacterMovePlan, CollisionWorld},
    protocol::{ActorMoveIntent, Position},
};

use super::{
    context::{ActorMoveContext, MoveCandidateResult, SelectedActorMove, horizontal_distance_sq},
    ordering::sorted_actor_plan_order,
    query::ActorMovementQuery,
};

// Actor behavior decides what an actor wants: simple patrol or a remembered go-to
// position. Patrol just rerolls when blocked; go-to movement uses the steering,
// wall-avoidance, and crowding logic needed to keep pursuing a real target.
pub(crate) fn plan_actor_moves(
    delta: f32,
    collision_world: &CollisionWorld,
    gameplay_config: &GameplayConfig,
    server_gameplay_config: &ServerGameplayConfig,
    players: &PlayerMap,
    actors: &mut ActorMap,
    actor_starts: &[(bevy::prelude::Entity, Position)],
    query: &mut ActorMovementQuery,
    planned_moves: &mut Vec<CharacterMovePlan>,
) {
    let mut rng = rng();
    let actor_order = sorted_actor_plan_order(query, actors);

    for actor_order in actor_order {
        let Ok((entity, id, pos, motion, mut move_intent, mut face_dir)) = query.get_mut(actor_order.entity) else {
            continue;
        };
        let Some(info) = actors.get_mut(id) else {
            continue;
        };
        // Per-kind config: cross-validated against the map at startup, so unwraps are safe.
        let actor_config = gameplay_config.validated_actor(&info.spawn_kind);
        let kind_server_config = server_gameplay_config.validated_actor(&info.spawn_kind);
        let actor_physics = actor_config.physics();
        let current_pos = *pos;
        info.move_intent_send_timer += delta;
        let go_to_intent = actor_desired_intent(
            &mut info.go_to_position,
            &current_pos,
            kind_server_config.go_to_reached_distance,
            actor_config.chase_speed,
        );
        let move_context = ActorMoveContext {
            entity,
            pos: &current_pos,
            vertical_velocity: motion.0,
            actor_physics,
            delta,
            collision_world,
            planned_moves,
            actor_starts,
            path_clear_lookahead_time: kind_server_config.path_clear_lookahead_time,
        };
        let selected_move = if let Some(go_to_intent) = go_to_intent {
            select_go_to_actor_move(&move_context, go_to_intent, info.go_to_position, info, &mut rng)
        } else {
            select_patrol_actor_move(&move_context, info, actor_config.patrol_speed, &mut rng)
        };

        *move_intent = selected_move.intent;
        if let Some(direction) = selected_move.intent.direction() {
            face_dir.0 = direction;
        }
        maybe_broadcast_actor_move_intent(players, *id, current_pos, selected_move.intent, motion.0, info);

        planned_moves.push(CharacterMovePlan::from_movement_result(
            entity,
            current_pos,
            selected_move.step,
            actor_physics,
        ));
    }
}

fn select_patrol_actor_move(
    context: &ActorMoveContext,
    info: &mut ActorInfo,
    patrol_speed: f32,
    rng: &mut ThreadRng,
) -> SelectedActorMove {
    info.wall_avoidance_direction = None;
    let patrol_intent = info.patrol_intent;
    if patrol_intent.direction().is_none() {
        return context.idle_move();
    }

    match context.evaluate_patrol_candidate(patrol_intent) {
        MoveCandidateResult::Accepted { selected } => selected,
        MoveCandidateResult::BlockedByCharacter | MoveCandidateResult::BlockedByWorld { .. } => {
            let next_patrol_intent = random_patrol_move_intent(rng, patrol_speed);
            info.patrol_intent = next_patrol_intent;
            match context.evaluate_patrol_candidate(next_patrol_intent) {
                MoveCandidateResult::Accepted { selected } => selected,
                MoveCandidateResult::BlockedByCharacter | MoveCandidateResult::BlockedByWorld { .. } => {
                    context.idle_move()
                }
            }
        }
    }
}

pub(super) fn select_go_to_actor_move(
    context: &ActorMoveContext,
    desired_intent: ActorMoveIntent,
    go_to_position: Option<Position>,
    info: &mut ActorInfo,
    rng: &mut ThreadRng,
) -> SelectedActorMove {
    let Some(direction) = desired_intent.direction() else {
        info.wall_avoidance_direction = None;
        return context.idle_move();
    };
    let speed = desired_intent
        .speed()
        .expect("moving actor intent should include speed");

    if let Some(selected_move) = continue_wall_avoidance_if_needed(context, direction, speed, go_to_position, info) {
        return selected_move;
    }

    match choose_steering_move(context, direction, speed, rng) {
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

    if let Some(selected_move) = choose_new_wall_avoidance_move(context, direction, speed, rng) {
        info.wall_avoidance_direction = selected_move.intent.direction();
        return selected_move;
    }

    context.idle_move()
}

pub(super) fn continue_wall_avoidance_if_needed(
    context: &ActorMoveContext,
    direction: f32,
    speed: f32,
    go_to_position: Option<Position>,
    info: &mut ActorInfo,
) -> Option<SelectedActorMove> {
    let avoidance_direction = info.wall_avoidance_direction?;
    if direct_path_is_clear_enough(context, direction, speed, go_to_position) {
        info.wall_avoidance_direction = None;
        return None;
    }

    let avoidance_intent = ActorMoveIntent::Moving {
        direction: avoidance_direction,
        speed,
    };
    match context.evaluate_candidate(avoidance_intent) {
        MoveCandidateResult::Accepted { selected } => Some(selected),
        MoveCandidateResult::BlockedByCharacter => Some(context.idle_move()),
        MoveCandidateResult::BlockedByWorld { .. } => {
            let next_move = choose_opposite_wall_avoidance_move(context, avoidance_direction, speed);
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

fn choose_steering_move(
    context: &ActorMoveContext,
    direction: f32,
    speed: f32,
    rng: &mut ThreadRng,
) -> SteeringMoveChoice {
    let mut was_blocked_by_character = false;
    let avoidance_side = random_avoidance_side(rng);
    for (index, candidate_direction) in steering_directions(direction, avoidance_side).into_iter().enumerate() {
        let candidate_intent = ActorMoveIntent::Moving {
            direction: candidate_direction,
            speed,
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

fn choose_new_wall_avoidance_move(
    context: &ActorMoveContext,
    direction: f32,
    speed: f32,
    rng: &mut ThreadRng,
) -> Option<SelectedActorMove> {
    let side = random_avoidance_side(rng);
    choose_wall_avoidance_move(context, wall_avoidance_directions(direction, side), speed)
}

fn choose_opposite_wall_avoidance_move(
    context: &ActorMoveContext,
    current_direction: f32,
    speed: f32,
) -> Option<SelectedActorMove> {
    choose_wall_avoidance_move(context, [opposite_wall_avoidance_direction(current_direction)], speed)
}

pub(super) fn wall_avoidance_directions(direction: f32, side: f32) -> [f32; 2] {
    [
        direction + side * std::f32::consts::FRAC_PI_2,
        direction - side * std::f32::consts::FRAC_PI_2,
    ]
}

pub(super) fn opposite_wall_avoidance_direction(current_direction: f32) -> f32 {
    current_direction + std::f32::consts::PI
}

fn choose_wall_avoidance_move<const N: usize>(
    context: &ActorMoveContext,
    directions: [f32; N],
    speed: f32,
) -> Option<SelectedActorMove> {
    // Wall avoidance is allowed to keep a blocked side-contact result only
    // when Rapier still moved the actor meaningfully. That lets actors slide
    // around geometry edges without accepting contact jitter as progress.
    for direction in directions {
        let intent = ActorMoveIntent::Moving { direction, speed };
        match context.evaluate_candidate(intent) {
            MoveCandidateResult::Accepted { selected } => return Some(selected),
            MoveCandidateResult::BlockedByWorld { selected }
                if blocked_step_made_useful_progress(context.pos, &selected.step.position, speed, context.delta) =>
            {
                return Some(selected);
            }
            MoveCandidateResult::BlockedByCharacter | MoveCandidateResult::BlockedByWorld { .. } => {}
        }
    }
    None
}

pub(super) fn blocked_step_made_useful_progress(
    start: &Position,
    target: &Position,
    actor_speed: f32,
    delta: f32,
) -> bool {
    let requested_distance = actor_speed * delta;
    let useful_distance = requested_distance * 0.5;
    horizontal_distance_sq(start, target) > useful_distance * useful_distance
}

fn direct_path_is_clear_enough(
    context: &ActorMoveContext,
    direction: f32,
    speed: f32,
    go_to_position: Option<Position>,
) -> bool {
    let direct_intent = ActorMoveIntent::Moving { direction, speed };
    let step = context.step_actor_move(direct_intent, context.path_clear_lookahead_time);
    if step.blocked {
        return false;
    }

    go_to_position.is_none_or(|target| {
        horizontal_distance_sq(&step.position, &target) < horizontal_distance_sq(context.pos, &target)
    })
}
