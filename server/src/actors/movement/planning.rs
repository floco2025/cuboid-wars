use std::f32::consts::TAU;

use crate::{
    config::ServerGameplayConfig,
    map::OpenBarrierKinds,
    network::broadcast_to_all,
    resources::{ActorInfo, ActorMap, PlayerMap},
};
use common::{
    config::{CharacterPhysicsConfig, GameplayConfig},
    physics::{CharacterMovePlan, CollisionWorld},
    protocol::{ActorId, ActorMoveIntent, ActorMovementState, Position, SActorMoveIntent, ServerMessage},
};

use super::{
    context::{ActorMoveContext, CandidateStep, SelectedActorMove, StepPolicy},
    ordering::sorted_actor_plan_order,
    query::ActorMovementQuery,
    steering::{ActorDesire, angular_distance, desired_move},
};

// Full-circle candidate resolution for movement selection. 12 → 30° steps:
// fine enough to slide around obstacles, cheap enough to evaluate per actor
// per tick (and only when the straight-at-goal fast path is blocked).
const AVOIDANCE_DIRECTION_COUNT: usize = 12;

// Continuity bias: a small reward for staying near last tick's heading, to damp
// oscillation at corners. Must stay well below 1 so it never overrides the
// goal-seeking term.
const AVOIDANCE_HYSTERESIS_WEIGHT: f32 = 0.2;

// Score penalty (radians) for a heading that only grazes a wall (slides) rather
// than running clear. A clean turn within this much of the grazing heading is
// preferred over the graze; a bigger detour is not.
const AVOIDANCE_GRAZE_PENALTY: f32 = 0.5;

// How long an actor commits to a chosen heading before re-deciding. Re-running
// the full selection every tick lets a jostling actor pick a different heading
// each tick — thrashing its position, facing, and broadcasts. Committing for a
// short window smooths the authoritative motion (so client prediction stays in
// sync) and caps `SActorMoveIntent` traffic, while still being frequent enough
// (~5×/s) to track a moving target. A blocked committed heading re-decides at once.
const STEERING_COMMIT_SECS: f32 = 0.2;

// The behavior layer decides what an actor wants (a go-to target when chasing
// or returning, otherwise a patrol heading); this layer turns that desire into
// an actual collision-free step via one unified direction-scoring selector.
pub(crate) fn plan_actor_moves(
    delta: f32,
    collision_world: &CollisionWorld,
    gameplay_config: &GameplayConfig,
    server_gameplay_config: &ServerGameplayConfig,
    players: &PlayerMap,
    open_barrier_kinds: &OpenBarrierKinds,
    actors: &mut ActorMap,
    actor_starts: &[(bevy::prelude::Entity, Position, CharacterPhysicsConfig)],
    query: &mut ActorMovementQuery,
    planned_moves: &mut Vec<CharacterMovePlan>,
) {
    let actor_order = sorted_actor_plan_order(query, actors);

    for actor_order in actor_order {
        let Ok((entity, id, pos, motion, mut move_intent, mut face_dir)) = query.get_mut(actor_order.entity) else {
            continue;
        };
        let Some(info) = actors.get_mut(id) else {
            continue;
        };
        // Per-kind config: actor kinds are cross-validated against the map at
        // startup, so `validated_actor` cannot panic here.
        let actor_config = gameplay_config.validated_actor(&info.spawn_kind);
        let kind_server_config = server_gameplay_config.validated_actor(&info.spawn_kind);
        let actor_physics = actor_config.physics();
        let current_pos = *pos;
        let reached_distance = kind_server_config.navigation.go_to_reached_distance;
        let move_context = ActorMoveContext {
            entity,
            pos: &current_pos,
            vertical_velocity: motion.0,
            actor_physics,
            delta,
            collision_world,
            planned_moves,
            actor_starts,
            path_clear_lookahead_secs: kind_server_config.navigation.path_clear_lookahead_secs,
            open_barrier_kinds: &open_barrier_kinds.0,
        };

        // Hysteresis anchor: where the actor was already heading.
        let last_direction = move_intent.direction();
        let selected_move = match desired_move(&info.goal, &current_pos, actor_config.chase_speed, reached_distance) {
            ActorDesire::Idle => {
                clear_commit(info);
                move_context.idle_move()
            }
            ActorDesire::Move { intent, policy } => {
                select_committed_move(info, &move_context, intent, last_direction, policy, delta)
            }
        };

        // The ECS `move_intent` holds the last-committed (last-broadcast)
        // value. Any change of any size is a real motion change (AI is
        // deterministic, no sensor noise) so broadcast on every difference.
        let committed_this_tick = *move_intent != selected_move.intent;
        *move_intent = selected_move.intent;
        if let Some(direction) = selected_move.intent.direction() {
            face_dir.0 = direction;
        }
        if committed_this_tick {
            broadcast_actor_move_intent(players, *id, current_pos, selected_move.intent, motion.0);
        }

        planned_moves.push(CharacterMovePlan::from_movement_result(
            entity,
            current_pos,
            selected_move.step,
            actor_physics,
        ));
    }
}

// Reuse the committed heading only while the commit window is live and that
// heading is still takeable this tick.
pub(super) fn should_reuse_commit(commit_secs_left: f32, committed_viable: bool) -> bool {
    commit_secs_left > 0.0 && committed_viable
}

fn clear_commit(info: &mut ActorInfo) {
    info.committed_direction = None;
    info.commit_secs_left = 0.0;
}

// Like `select_actor_move`, but commits to the chosen heading for
// `STEERING_COMMIT_SECS`: while the window is live and the committed heading is
// still collision-free (re-evaluated against the current position), reuse it
// instead of re-selecting. Re-decides — starting a fresh window — when the
// commit lapses or the heading becomes blocked. Committing the *decision* (not
// just the broadcast) keeps the simulated path smooth, so client prediction
// stays in sync and `SActorMoveIntent` traffic stays sparse.
//
// Only the *direction* is committed; the step always uses the current desired
// speed, so a chase commit (chase speed) can't carry that speed into patrol.
pub(super) fn select_committed_move(
    info: &mut ActorInfo,
    context: &ActorMoveContext,
    desired: ActorMoveIntent,
    last_direction: Option<f32>,
    policy: StepPolicy,
    delta: f32,
) -> SelectedActorMove {
    info.commit_secs_left -= delta;
    let desired_speed = desired.speed().expect("desired move intent has no speed");
    let committed_step = info
        .committed_direction
        .and_then(|direction| candidate_move(context, direction, desired_speed, policy));
    if should_reuse_commit(info.commit_secs_left, committed_step.is_some()) {
        return committed_step.expect("reuse requires a viable committed step").selected;
    }
    let selected = select_actor_move(context, desired, last_direction, policy);
    info.committed_direction = selected.intent.direction();
    info.commit_secs_left = STEERING_COMMIT_SECS;
    selected
}

// Choose this tick's step for an actor that wants to move along `desired`.
//
// One mechanism for every situation: consider the full circle of headings
// around the desired direction and take the best collision-free one — least
// turn from the goal, biased toward last tick's heading for continuity, with a
// consistent handed tie-break so two head-on actors yield to the same side and
// pass instead of deadlocking. A clear shot at the goal is the fast path. Only
// when every candidate is blocked (genuinely boxed in) does the actor idle.
//
// `StepPolicy::Strict` (patrol) rejects steps that would walk off an edge and
// never grazes walls; `Pursue` follows targets off ledges and may graze along
// a wall when it still makes useful progress.
pub(super) fn select_actor_move(
    context: &ActorMoveContext,
    desired: ActorMoveIntent,
    last_direction: Option<f32>,
    policy: StepPolicy,
) -> SelectedActorMove {
    let desired_direction = desired.direction().expect("desired move intent has no direction");
    let speed = desired.speed().expect("desired move intent has no speed");

    // Fast path: a clear straight shot at the goal always wins the score.
    if let Some(selected) = candidate_move(context, desired_direction, speed, policy).filter(|c| c.clean) {
        return selected.selected;
    }

    let step = TAU / AVOIDANCE_DIRECTION_COUNT as f32;
    let mut best: Option<(f32, f32, SelectedActorMove)> = None;
    for i in 0..AVOIDANCE_DIRECTION_COUNT {
        // Offsets fan out in pairs: 0, +step, -step, +2·step, -2·step, …
        let pair = i.div_ceil(2);
        let sign = if i % 2 == 1 { 1.0 } else { -1.0 };
        let offset = sign * pair as f32 * step;
        let direction = desired_direction + offset;

        let Some(candidate) = candidate_move(context, direction, speed, policy) else {
            continue;
        };
        let mut score = -angular_distance(direction, desired_direction);
        if let Some(last) = last_direction {
            score -= AVOIDANCE_HYSTERESIS_WEIGHT * angular_distance(direction, last);
        }
        if !candidate.clean {
            score -= AVOIDANCE_GRAZE_PENALTY;
        }
        // Tie-break on `offset` (higher wins) gives a consistent turn handedness
        // so mirror-image actors don't pick conflicting mirror sidesteps.
        if best
            .as_ref()
            .is_none_or(|(best_score, best_offset, _)| (score, offset) > (*best_score, *best_offset))
        {
            best = Some((score, offset, candidate.selected));
        }
    }

    best.map_or_else(|| context.idle_move(), |(_, _, selected)| selected)
}

struct Candidate {
    selected: SelectedActorMove,
    // True when the step runs clear; false when it only grazes a wall (slides).
    clean: bool,
}

// Evaluate one heading. `None` if the actor can't take it per the policy.
fn candidate_move(context: &ActorMoveContext, direction: f32, speed: f32, policy: StepPolicy) -> Option<Candidate> {
    let intent = ActorMoveIntent::Moving { direction, speed };
    match context.evaluate(intent, policy) {
        CandidateStep::Clear(selected) => Some(Candidate { selected, clean: true }),
        CandidateStep::Graze(selected) => Some(Candidate { selected, clean: false }),
        CandidateStep::Blocked => None,
    }
}

fn broadcast_actor_move_intent(
    players: &PlayerMap,
    id: ActorId,
    pos: Position,
    move_intent: ActorMoveIntent,
    vertical_velocity: f32,
) {
    broadcast_to_all(
        players,
        ServerMessage::ActorMoveIntent(SActorMoveIntent {
            id,
            movement: ActorMovementState::new(pos, move_intent, vertical_velocity),
        }),
    );
}
