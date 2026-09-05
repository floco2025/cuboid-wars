use bevy::prelude::{Entity, Vec3};
use common::{
    config::CharacterPhysicsConfig,
    map::MovingFloors,
    physics::{
        CharacterEnvironment, CharacterMovePlan, CharacterMovementResult, CharacterStep, CharacterSupport,
        CollisionWorld, character_move_plan_is_blocked, step_character_movement,
    },
    protocol::{ActorMoveIntent, BarrierKindId, Position},
};

#[derive(Copy, Clone)]
pub(super) struct SelectedActorMove {
    pub(super) intent: ActorMoveIntent,
    pub(super) step: CharacterMovementResult,
}

pub(super) enum CandidateStep {
    Clear(SelectedActorMove),
    Graze(SelectedActorMove),
    CharacterBlocked,
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
    pub(super) open_barrier_kinds: &'a [BarrierKindId],
    pub(super) gravity: f32,
    pub(super) ladder_climb_ratio: f32,
    pub(super) knockback_step: Vec3,
    pub(super) moving_floors: &'a MovingFloors,
}

impl ActorMoveContext<'_> {
    pub(super) fn idle_move(&self) -> SelectedActorMove {
        let intent = ActorMoveIntent::Idle;
        SelectedActorMove {
            intent,
            step: self.step_actor_move(intent),
        }
    }

    pub(super) fn evaluate(&self, intent: ActorMoveIntent, target: &Position, require_support: bool) -> CandidateStep {
        let selected = SelectedActorMove {
            intent,
            step: self.step_actor_move(intent),
        };
        let planned =
            CharacterMovePlan::from_movement_result(self.entity, *self.pos, selected.step, self.actor_physics);
        if character_move_plan_is_blocked(&planned, self.planned_moves, self.actor_starts) {
            return CandidateStep::CharacterBlocked;
        }
        if require_support && selected.step.support == CharacterSupport::Airborne {
            return CandidateStep::Blocked;
        }
        if !selected.step.blocked {
            return CandidateStep::Clear(selected);
        }
        if blocked_step_made_useful_progress(
            self.pos,
            &selected.step.position,
            intent.speed().unwrap_or(0.0),
            self.delta,
        ) && selected.step.position.horizontal_distance_sq(target) < self.pos.horizontal_distance_sq(target)
        {
            CandidateStep::Graze(selected)
        } else {
            CandidateStep::Blocked
        }
    }

    fn step_actor_move(&self, move_intent: ActorMoveIntent) -> CharacterMovementResult {
        step_character_movement(
            CharacterStep {
                start: *self.pos,
                vertical_velocity: self.vertical_velocity,
                control_velocity: move_intent.to_horizontal_velocity(),
                external_displacement: self.knockback_step,
                delta: self.delta,
            },
            &CharacterEnvironment {
                collision_world: self.collision_world,
                gravity: self.gravity,
                passable_kinds: self.open_barrier_kinds,
                physics: self.actor_physics,
                ladder_climb_ratio: self.ladder_climb_ratio,
                portals: None,
                moving_floors: self.moving_floors,
            },
        )
    }
}

pub(super) fn blocked_step_made_useful_progress(
    start: &Position,
    target: &Position,
    actor_speed: f32,
    delta: f32,
) -> bool {
    let useful_distance = actor_speed * delta * 0.5;
    start.horizontal_distance_sq(target) > useful_distance * useful_distance
}
