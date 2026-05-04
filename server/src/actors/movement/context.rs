use bevy::prelude::Entity;
use common::{
    config::CharacterPhysicsConfig,
    physics::{
        CharacterMovePlan, CharacterMovementResult, CollisionWorld, character_move_plan_is_blocked,
        position_has_floor_support, step_character_movement,
    },
    protocol::{ActorMoveIntent, Position},
};

#[derive(Copy, Clone)]
pub(super) struct SelectedActorMove {
    pub(super) intent: ActorMoveIntent,
    pub(super) step: CharacterMovementResult,
}

pub(super) struct ActorMoveContext<'a> {
    pub(super) entity: Entity,
    pub(super) pos: &'a Position,
    pub(super) vertical_velocity: f32,
    pub(super) actor_physics: CharacterPhysicsConfig,
    pub(super) delta: f32,
    pub(super) collision_world: &'a CollisionWorld,
    pub(super) planned_moves: &'a [CharacterMovePlan],
    pub(super) actor_starts: &'a [(Entity, Position)],
    pub(super) path_clear_lookahead_time: f32,
}

impl ActorMoveContext<'_> {
    pub(super) fn idle_move(&self) -> SelectedActorMove {
        let intent = ActorMoveIntent::Idle;
        SelectedActorMove {
            intent,
            step: self.step_actor_move(intent, self.delta),
        }
    }

    pub(super) fn step_actor_move(&self, move_intent: ActorMoveIntent, delta: f32) -> CharacterMovementResult {
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

    pub(super) fn evaluate_candidate(&self, intent: ActorMoveIntent) -> MoveCandidateResult {
        let selected = SelectedActorMove {
            intent,
            step: self.step_actor_move(intent, self.delta),
        };
        let planned_move =
            CharacterMovePlan::from_movement_result(self.entity, *self.pos, selected.step, self.actor_physics);

        if character_move_plan_is_blocked(&planned_move, self.planned_moves, self.actor_starts) {
            return MoveCandidateResult::BlockedByCharacter;
        }
        if selected.step.blocked {
            return MoveCandidateResult::BlockedByWorld { selected };
        }

        MoveCandidateResult::Accepted { selected }
    }

    // Patrol-only wrapper around `evaluate_candidate` that additionally treats
    // ledges as blocking. If the candidate is otherwise accepted but the
    // `path_clear_lookahead_time`-second projection of this intent lands on a
    // position with no floor underneath, demote it to `BlockedByWorld` so the
    // patrol selection rerolls a different direction. Chase intentionally does
    // not use this — chasers may follow a player off ledges.
    pub(super) fn evaluate_patrol_candidate(&self, intent: ActorMoveIntent) -> MoveCandidateResult {
        match self.evaluate_candidate(intent) {
            MoveCandidateResult::Accepted { selected } => {
                if self.patrol_step_lands_on_floor(intent) {
                    MoveCandidateResult::Accepted { selected }
                } else {
                    MoveCandidateResult::BlockedByWorld { selected }
                }
            }
            other => other,
        }
    }

    // Project the candidate intent forward by `path_clear_lookahead_time`,
    // keeping y constant, and ask whether that projected (x, z) still has
    // floor support. Y is preserved because the question is "would the ground
    // still be there if I stepped sideways," not "where would I land after
    // falling."
    fn patrol_step_lands_on_floor(&self, intent: ActorMoveIntent) -> bool {
        let velocity = intent.to_horizontal_velocity();
        let lookahead = self.path_clear_lookahead_time;
        let projected = Position {
            x: velocity.x.mul_add(lookahead, self.pos.x),
            y: self.pos.y,
            z: velocity.z.mul_add(lookahead, self.pos.z),
        };
        position_has_floor_support(self.collision_world, &projected, self.actor_physics)
    }
}

pub(super) enum MoveCandidateResult {
    Accepted { selected: SelectedActorMove },
    BlockedByCharacter,
    BlockedByWorld { selected: SelectedActorMove },
}

pub(super) fn horizontal_distance_sq(a: &Position, b: &Position) -> f32 {
    let dx = a.x - b.x;
    let dz = a.z - b.z;
    dx.mul_add(dx, dz * dz)
}
