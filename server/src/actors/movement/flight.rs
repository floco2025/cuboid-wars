use super::{
    character_move_plan_is_blocked,
    context::{ActorMoveContext, SelectedActorMove},
};
use crate::actors::{ActorInfo, ActorMode};
use bevy::prelude::Vec3;
use common::{physics::CharacterMovePlan, protocol::ActorMoveIntent};

pub(super) fn select_flying_move(
    context: &ActorMoveContext<'_>,
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

fn evaluate(context: &ActorMoveContext<'_>, velocity: Vec3) -> Option<SelectedActorMove> {
    let selected = flying_step(context, velocity);
    let plan =
        CharacterMovePlan::from_movement_result(context.entity, *context.pos, selected.step, context.actor_physics);
    (!character_move_plan_is_blocked(&plan, context.planned_moves, context.actor_starts)).then_some(selected)
}

fn flying_step(context: &ActorMoveContext<'_>, velocity: Vec3) -> SelectedActorMove {
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
            context.open_barriers,
            context.carriers,
        ),
    }
}

#[cfg(test)]
#[path = "tests/flight.rs"]
mod tests;
