use bevy::prelude::*;
use rand::{rng, rngs::ThreadRng};

use super::{
    behavior::random_patrol_move_intent,
    network::maybe_broadcast_actor_move_intent,
    steering::{actor_desired_intent, random_avoidance_side, steering_directions},
};
use crate::{
    config::ServerGameplayConfig,
    resources::{ActorInfo, ActorMap, PlayerMap},
};
use common::{
    config::{CharacterPhysicsConfig, GameplayConfig},
    markers::{ActorMarker, PlayerMarker},
    physics::{
        CharacterMovePlan, CharacterMovementResult, CharacterVerticalVelocity, CollisionWorld,
        blocking_character_move_plan, character_move_plan_is_blocked, step_character_movement,
    },
    protocol::{ActorId, ActorMoveIntent, FaceDirection, Position},
};

// Actor behavior decides what an actor wants: simple patrol or a remembered go-to
// position. Patrol just rerolls when blocked; go-to movement uses the steering,
// wall-avoidance, and crowding logic needed to keep pursuing a real target.

pub(crate) type ActorMovementQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ActorId,
        &'static mut Position,
        &'static mut CharacterVerticalVelocity,
        &'static mut ActorMoveIntent,
        &'static mut FaceDirection,
    ),
    (With<ActorMarker>, Without<PlayerMarker>),
>;

pub(crate) fn plan_actor_moves(
    delta: f32,
    collision_world: &CollisionWorld,
    gameplay_config: &GameplayConfig,
    server_gameplay_config: &ServerGameplayConfig,
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
        let current_pos = *pos;
        info.move_intent_send_timer += delta;
        let go_to_intent = actor_desired_intent(
            &mut info.go_to_position,
            &current_pos,
            server_gameplay_config.actors.go_to_reached_distance,
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
            direct_path_probe_time: server_gameplay_config.actors.direct_path_probe_time,
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

        planned_moves.push(CharacterMovePlan {
            entity,
            start: current_pos,
            target: selected_move.step.position,
            target_vertical_velocity: selected_move.step.vertical_velocity,
            physics: actor_physics,
            blocked: selected_move.step.blocked,
        });
    }
}

#[derive(Copy, Clone)]
struct SelectedActorMove {
    intent: ActorMoveIntent,
    step: CharacterMovementResult,
}

struct ActorMoveContext<'a> {
    entity: Entity,
    pos: &'a Position,
    vertical_velocity: f32,
    actor_physics: CharacterPhysicsConfig,
    delta: f32,
    collision_world: &'a CollisionWorld,
    planned_moves: &'a [CharacterMovePlan],
    actor_starts: &'a [(Entity, Position)],
    direct_path_probe_time: f32,
}

impl ActorMoveContext<'_> {
    fn idle_move(&self) -> SelectedActorMove {
        let intent = ActorMoveIntent::Idle;
        SelectedActorMove {
            intent,
            step: self.step_actor_move(intent, self.delta),
        }
    }

    fn step_actor_move(&self, move_intent: ActorMoveIntent, delta: f32) -> CharacterMovementResult {
        let velocity = move_intent.to_horizontal_velocity();
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

    fn evaluate_candidate(&self, intent: ActorMoveIntent) -> MoveCandidateResult {
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

    match context.evaluate_candidate(patrol_intent) {
        MoveCandidateResult::Accepted { selected } => selected,
        MoveCandidateResult::BlockedByCharacter | MoveCandidateResult::BlockedByWorld { .. } => {
            let next_patrol_intent = random_patrol_move_intent(rng, patrol_speed);
            info.patrol_intent = next_patrol_intent;
            match context.evaluate_candidate(next_patrol_intent) {
                MoveCandidateResult::Accepted { selected } => selected,
                MoveCandidateResult::BlockedByCharacter | MoveCandidateResult::BlockedByWorld { .. } => {
                    context.idle_move()
                }
            }
        }
    }
}

fn select_go_to_actor_move(
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

fn continue_wall_avoidance_if_needed(
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

enum MoveCandidateResult {
    Accepted { selected: SelectedActorMove },
    BlockedByCharacter,
    BlockedByWorld { selected: SelectedActorMove },
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

fn blocked_step_made_useful_progress(start: &Position, target: &Position, actor_speed: f32, delta: f32) -> bool {
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
    let step = context.step_actor_move(direct_intent, context.direct_path_probe_time);
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
    use common::{
        constants::{FLOOR_THICKNESS, WALL_THICKNESS},
        protocol::{ActorKind, Floor, MapLayout, Wall},
    };

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

    fn actor_info() -> ActorInfo {
        ActorInfo {
            entity: test_entity(1),
            kind: ActorKind::Automaton,
            direction_timer: 0.0,
            patrol_intent: ActorMoveIntent::Idle,
            go_to_position: None,
            wall_avoidance_direction: None,
            last_broadcast_move_intent: ActorMoveIntent::Idle,
            move_intent_send_timer: 0.0,
        }
    }

    fn actor_physics() -> CharacterPhysicsConfig {
        GameplayConfig::load_default()
            .expect("default gameplay config should load")
            .characters
            .actor
            .physics()
    }

    fn actor_speed() -> f32 {
        GameplayConfig::load_default()
            .expect("default gameplay config should load")
            .characters
            .actor
            .patrol_speed
    }

    fn test_entity(index: u64) -> Entity {
        Entity::from_bits(index)
    }

    fn floor() -> Floor {
        Floor {
            x1: -4.0,
            z1: -4.0,
            x2: 4.0,
            z2: 4.0,
            y: 0.0,
            thickness: FLOOR_THICKNESS,
            level: 0,
        }
    }

    fn wall() -> Wall {
        Wall {
            x1: 0.0,
            z1: -2.0,
            x2: 0.0,
            z2: 2.0,
            width: WALL_THICKNESS,
            level: 0,
        }
    }

    fn collision_world(walls: &[Wall]) -> CollisionWorld {
        CollisionWorld::from_map_layout(&MapLayout {
            walls: walls.to_vec(),
            ramps: vec![],
            floors: vec![floor()],
            wall_lights: vec![],
        })
    }

    fn context<'a>(
        entity: Entity,
        pos: &'a Position,
        collision_world: &'a CollisionWorld,
        planned_moves: &'a [CharacterMovePlan],
        actor_starts: &'a [(Entity, Position)],
    ) -> ActorMoveContext<'a> {
        ActorMoveContext {
            entity,
            pos,
            vertical_velocity: 0.0,
            actor_physics: actor_physics(),
            delta: 0.1,
            collision_world,
            planned_moves,
            actor_starts,
            direct_path_probe_time: 0.4,
        }
    }

    fn planned_move(entity: Entity, start: Position, target: Position) -> CharacterMovePlan {
        CharacterMovePlan {
            entity,
            start,
            target,
            target_vertical_velocity: 0.0,
            physics: actor_physics(),
            blocked: false,
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
            patrol_intent: ActorMoveIntent::Idle,
            go_to_position: Some(Position { x: 1.0, y: 0.0, z: 0.0 }),
            wall_avoidance_direction: None,
            last_broadcast_move_intent: ActorMoveIntent::Idle,
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

    #[test]
    fn candidate_blocked_by_static_world_is_not_character_blocked() {
        let pos = Position {
            x: -1.0,
            y: 0.0,
            z: 0.0,
        };
        let collision_world = collision_world(&[wall()]);
        let planned_moves = [];
        let actor_starts = [];
        let context = context(test_entity(1), &pos, &collision_world, &planned_moves, &actor_starts);
        let intent = ActorMoveIntent::Moving {
            direction: std::f32::consts::FRAC_PI_2,
            speed: 20.0,
        };

        match context.evaluate_candidate(intent) {
            MoveCandidateResult::BlockedByWorld { selected } => {
                assert!(selected.step.blocked);
                assert_eq!(selected.intent, intent);
            }
            MoveCandidateResult::Accepted { .. } | MoveCandidateResult::BlockedByCharacter => {
                panic!("expected static-world block")
            }
        }
    }

    #[test]
    fn candidate_blocked_by_other_character_yields_before_wall_avoidance() {
        let pos = Position::default();
        let collision_world = collision_world(&[]);
        let planned_moves = [];
        let actor_starts = [(test_entity(2), Position { x: 0.6, y: 0.0, z: 0.0 })];
        let context = context(test_entity(1), &pos, &collision_world, &planned_moves, &actor_starts);
        let intent = ActorMoveIntent::Moving {
            direction: std::f32::consts::FRAC_PI_2,
            speed: actor_speed(),
        };

        match context.evaluate_candidate(intent) {
            MoveCandidateResult::BlockedByCharacter => {}
            MoveCandidateResult::Accepted { .. } | MoveCandidateResult::BlockedByWorld { .. } => {
                panic!("expected character block")
            }
        }
    }

    #[test]
    fn character_blocked_steering_selects_idle_when_all_steering_options_are_blocked() {
        let pos = Position::default();
        let collision_world = collision_world(&[]);
        let planned_moves = [];
        let base_direction = std::f32::consts::FRAC_PI_2;
        let blocked_positions = [
            base_direction,
            base_direction + 20.0_f32.to_radians(),
            base_direction + 45.0_f32.to_radians(),
            base_direction + 90.0_f32.to_radians(),
            base_direction - 20.0_f32.to_radians(),
            base_direction - 45.0_f32.to_radians(),
            base_direction - 90.0_f32.to_radians(),
        ];
        let actor_starts: Vec<_> = blocked_positions
            .into_iter()
            .enumerate()
            .map(|(index, direction)| {
                (
                    test_entity(index as u64 + 2),
                    Position {
                        x: direction.sin() * 0.6,
                        y: 0.0,
                        z: direction.cos() * 0.6,
                    },
                )
            })
            .collect();
        let context = context(test_entity(1), &pos, &collision_world, &planned_moves, &actor_starts);
        let mut info = actor_info();
        let mut rng = rng();

        let selected = select_go_to_actor_move(
            &context,
            ActorMoveIntent::Moving {
                direction: base_direction,
                speed: actor_speed(),
            },
            None,
            &mut info,
            &mut rng,
        );

        assert_eq!(selected.intent, ActorMoveIntent::Idle);
        assert_eq!(info.wall_avoidance_direction, None);
    }

    #[test]
    fn wall_avoidance_clears_when_direct_path_is_clear_again() {
        let pos = Position::default();
        let collision_world = collision_world(&[]);
        let planned_moves = [];
        let actor_starts = [];
        let context = context(test_entity(1), &pos, &collision_world, &planned_moves, &actor_starts);
        let mut info = actor_info();
        info.wall_avoidance_direction = Some(std::f32::consts::FRAC_PI_2);

        let selected = continue_wall_avoidance_if_needed(
            &context,
            0.0,
            actor_speed(),
            Some(Position { x: 0.0, y: 0.0, z: 5.0 }),
            &mut info,
        );

        assert!(selected.is_none());
        assert_eq!(info.wall_avoidance_direction, None);
    }

    #[test]
    fn committed_wall_avoidance_yields_when_blocked_by_character() {
        let pos = Position {
            x: -1.0,
            y: 0.0,
            z: 0.0,
        };
        let collision_world = collision_world(&[wall()]);
        let planned_moves = [];
        let actor_starts = [(
            test_entity(2),
            Position {
                x: -1.0,
                y: 0.0,
                z: 0.6,
            },
        )];
        let context = context(test_entity(1), &pos, &collision_world, &planned_moves, &actor_starts);
        let mut info = actor_info();
        info.wall_avoidance_direction = Some(0.0);

        let selected = continue_wall_avoidance_if_needed(
            &context,
            std::f32::consts::FRAC_PI_2,
            actor_speed(),
            Some(Position { x: 5.0, y: 0.0, z: 0.0 }),
            &mut info,
        )
        .expect("wall avoidance should yield with an idle move");

        assert_eq!(selected.intent, ActorMoveIntent::Idle);
        assert_eq!(info.wall_avoidance_direction, Some(0.0));
    }

    #[test]
    fn following_front_actor_is_not_blocked_when_final_positions_do_not_overlap() {
        let pos = Position::default();
        let collision_world = collision_world(&[]);
        let front_start = Position { x: 1.2, y: 0.0, z: 0.0 };
        let planned_moves = [planned_move(
            test_entity(2),
            front_start,
            Position { x: 2.0, y: 0.0, z: 0.0 },
        )];
        let actor_starts = [(test_entity(2), front_start)];
        let context = context(test_entity(1), &pos, &collision_world, &planned_moves, &actor_starts);
        let intent = ActorMoveIntent::Moving {
            direction: std::f32::consts::FRAC_PI_2,
            speed: actor_speed(),
        };

        match context.evaluate_candidate(intent) {
            MoveCandidateResult::Accepted { .. } => {}
            MoveCandidateResult::BlockedByCharacter | MoveCandidateResult::BlockedByWorld { .. } => {
                panic!("expected following move to be accepted")
            }
        }
    }
}
