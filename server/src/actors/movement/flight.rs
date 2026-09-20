use super::character_move_plan_is_blocked;
use crate::actors::{ActorInfo, ActorMode};
use bevy::prelude::{Entity, Vec3};
use common::{
    config::CharacterPhysicsConfig,
    map::Carriers,
    physics::{CharacterMovePlan, CharacterMovementResult, CollisionWorld},
    protocol::{ActorMoveIntent, FieldId, Position},
};

pub(super) struct FlightMoveContext<'a> {
    pub(super) entity: Entity,
    pub(super) pos: &'a Position,
    pub(super) actor_physics: CharacterPhysicsConfig,
    pub(super) delta: f32,
    pub(super) collision_world: &'a CollisionWorld,
    pub(super) planned_moves: &'a [CharacterMovePlan],
    pub(super) actor_starts: &'a [(Entity, Position, CharacterPhysicsConfig)],
    pub(super) open_fields: &'a [FieldId],
    pub(super) knockback_step: Vec3,
    pub(super) carriers: &'a Carriers,
}

pub(super) struct SelectedActorMove {
    pub(super) intent: ActorMoveIntent,
    pub(super) step: CharacterMovementResult,
}

pub(super) fn select_flying_move(
    context: &FlightMoveContext<'_>,
    info: &ActorInfo,
    roam_speed: f32,
    active_speed: f32,
) -> SelectedActorMove {
    let target = info.flight.as_ref().and_then(|flight| flight.route.front()).copied();
    let speed = if matches!(info.mode, ActorMode::Roam) {
        roam_speed
    } else {
        active_speed
    };
    let velocity = target.map_or(Vec3::ZERO, |target| {
        let offset = Vec3::from(target) - Vec3::from(*context.pos);
        offset.normalize_or_zero() * speed.min(offset.length() / context.delta)
    });
    if let Some(selected) = evaluate(context, velocity) {
        return selected;
    }
    if let Some(target) = target
        && !matches!(info.mode, ActorMode::Roam)
    {
        let forward = velocity.normalize_or_zero();
        let lateral = forward.cross(Vec3::Y).try_normalize().unwrap_or(Vec3::X);
        let vertical = forward.cross(lateral).normalize_or_zero();
        for direction in [lateral, -lateral, vertical, -vertical] {
            let candidate = (forward + direction).normalize_or_zero() * speed;
            if let Some(selected) = evaluate(context, candidate)
                && selected.step.position.distance_sq(&target) < context.pos.distance_sq(&target)
            {
                return selected;
            }
        }
    }
    evaluate(context, Vec3::ZERO).unwrap_or_else(|| flying_step(context, Vec3::ZERO))
}

fn evaluate(context: &FlightMoveContext<'_>, velocity: Vec3) -> Option<SelectedActorMove> {
    let selected = flying_step(context, velocity);
    let plan =
        CharacterMovePlan::from_movement_result(context.entity, *context.pos, selected.step, context.actor_physics);
    (!character_move_plan_is_blocked(&plan, context.planned_moves, context.actor_starts)).then_some(selected)
}

fn flying_step(context: &FlightMoveContext<'_>, velocity: Vec3) -> SelectedActorMove {
    let translation = velocity * context.delta + context.knockback_step;
    SelectedActorMove {
        intent: ActorMoveIntent::Flying {
            velocity: velocity.to_array(),
        },
        step: context.collision_world.move_flying_character(
            *context.pos,
            translation,
            context.delta,
            context.actor_physics,
            context.open_fields,
            context.carriers,
        ),
    }
}

#[cfg(test)]
#[path = "tests/flight.rs"]
mod tests;
