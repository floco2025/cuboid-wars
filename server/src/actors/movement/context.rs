use bevy::prelude::{Entity, Vec3};
use common::{
    config::CharacterPhysicsConfig,
    physics::{
        CharacterEnvironment, CharacterMovePlan, CharacterMovementResult, CharacterStep, CollisionWorld,
        character_move_plan_is_blocked, position_has_floor_support, step_character_movement,
    },
    protocol::{ActorMoveIntent, BarrierKindId, Position},
};

#[derive(Copy, Clone)]
pub(super) struct SelectedActorMove {
    pub(super) intent: ActorMoveIntent,
    pub(super) step: CharacterMovementResult,
}

// How strictly a candidate step is judged. Patrol wanders and must stay safe;
// everything goal-directed may take riskier steps to make progress.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(super) enum StepPolicy {
    // Ledge-aware, never grazes walls (patrol; suspended during the
    // post-stall escape window).
    Strict,
    // May follow targets off ledges and graze along walls when the slide
    // still makes useful progress (chase / pursuit / return / escape).
    Pursue,
}

pub(super) enum CandidateStep {
    // Runs clear.
    Clear(SelectedActorMove),
    // World-blocked but slid far enough to be useful (Pursue only).
    Graze(SelectedActorMove),
    Blocked,
}

pub(super) struct ActorMoveContext<'a> {
    pub(super) entity: Entity,
    pub(super) pos: &'a Position,
    pub(super) vertical_velocity: f32,
    pub(super) actor_physics: CharacterPhysicsConfig,
    pub(super) delta: f32,
    pub(super) collision_world: &'a CollisionWorld,
    pub(super) planned_moves: &'a [CharacterMovePlan],
    pub(super) actor_starts: &'a [(Entity, Position, CharacterPhysicsConfig)],
    pub(super) path_clear_lookahead_secs: f32,
    // Pressure-plate-open barrier kinds — actors pass through these.
    pub(super) open_barrier_kinds: &'a [BarrierKindId],
    // Per-map gravity; actors never have low-gravity.
    pub(super) gravity: f32,
    pub(super) knockback_step: Vec3,
}

impl ActorMoveContext<'_> {
    pub(super) fn idle_move(&self) -> SelectedActorMove {
        let intent = ActorMoveIntent::Idle;
        SelectedActorMove {
            intent,
            step: self.step_actor_move(intent),
        }
    }

    pub(super) fn step_actor_move(&self, move_intent: ActorMoveIntent) -> CharacterMovementResult {
        let velocity = move_intent.to_horizontal_velocity();
        let target_x = velocity.x.mul_add(self.delta, self.pos.x) + self.knockback_step.x;
        let target_z = velocity.z.mul_add(self.delta, self.pos.z) + self.knockback_step.z;
        step_character_movement(
            CharacterStep {
                start: *self.pos,
                vertical_velocity: self.vertical_velocity,
                target_x,
                target_z,
                delta: self.delta,
            },
            &CharacterEnvironment {
                collision_world: self.collision_world,
                gravity: self.gravity,
                passable_kinds: self.open_barrier_kinds,
                physics: self.actor_physics,
            },
        )
    }

    // The single candidate-evaluation path. Character collisions always
    // block. World collisions block, except a Pursue step that still slid
    // usefully counts as a graze. An otherwise-clear Strict step that walks
    // off a ledge blocks — patrol rerolls a different direction instead;
    // Pursue intentionally follows targets off ledges.
    pub(super) fn evaluate(&self, intent: ActorMoveIntent, policy: StepPolicy) -> CandidateStep {
        let selected = SelectedActorMove {
            intent,
            step: self.step_actor_move(intent),
        };
        let planned_move =
            CharacterMovePlan::from_movement_result(self.entity, *self.pos, selected.step, self.actor_physics);

        if character_move_plan_is_blocked(&planned_move, self.planned_moves, self.actor_starts) {
            return CandidateStep::Blocked;
        }
        if selected.step.blocked {
            if policy == StepPolicy::Pursue
                && blocked_step_made_useful_progress(
                    self.pos,
                    &selected.step.position,
                    intent.speed().unwrap_or(0.0),
                    self.delta,
                )
            {
                return CandidateStep::Graze(selected);
            }
            return CandidateStep::Blocked;
        }
        if policy == StepPolicy::Strict && !self.patrol_step_lands_on_floor(intent) {
            return CandidateStep::Blocked;
        }

        CandidateStep::Clear(selected)
    }

    // Project the candidate intent forward by `path_clear_lookahead_secs`,
    // keeping y constant, and ask whether that projected (x, z) still has
    // floor support. Y is preserved because the question is "would the ground
    // still be there if I stepped sideways," not "where would I land after
    // falling."
    fn patrol_step_lands_on_floor(&self, intent: ActorMoveIntent) -> bool {
        let velocity = intent.to_horizontal_velocity();
        let lookahead = self.path_clear_lookahead_secs;
        let projected = Position {
            x: velocity.x.mul_add(lookahead, self.pos.x),
            y: self.pos.y,
            z: velocity.z.mul_add(lookahead, self.pos.z),
        };
        position_has_floor_support(self.collision_world, &projected, self.actor_physics)
    }
}

// True when a blocked step still moved the actor more than half the requested
// distance — enough to count as sliding along a wall rather than stalling.
pub(super) fn blocked_step_made_useful_progress(
    start: &Position,
    target: &Position,
    actor_speed: f32,
    delta: f32,
) -> bool {
    let requested_distance = actor_speed * delta;
    let useful_distance = requested_distance * 0.5;
    start.horizontal_distance_sq(target) > useful_distance * useful_distance
}
