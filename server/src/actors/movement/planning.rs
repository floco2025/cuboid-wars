use std::f32::consts::FRAC_PI_2;

use bevy::prelude::{Commands, Vec3};

use crate::actors::{ActorMap, ActorMode};
use common::{
    config::CharacterPhysicsConfig,
    map::Carriers,
    physics::{CharacterMovePlan, CharacterSupport, CollisionWorld},
    protocol::{ActorMoveIntent, MapSettings, PlateState, Position},
};

use super::{
    context::{ActorMoveContext, CandidateStep, SelectedActorMove},
    ordering::sorted_actor_plan_order,
    query::ActorMovementQuery,
    steering::{ActorDesire, desired_move, direction_toward},
};

#[allow(clippy::too_many_arguments)]
pub(crate) fn plan_actor_moves(
    delta: f32,
    collision_world: &CollisionWorld,
    map_settings: &MapSettings,
    commands: &mut Commands,
    plates: &PlateState,
    carriers: &Carriers,
    actors: &ActorMap,
    actor_starts: &[(bevy::prelude::Entity, Position, CharacterPhysicsConfig)],
    query: &mut ActorMovementQuery,
    planned_moves: &mut Vec<CharacterMovePlan>,
) {
    let actor_order = sorted_actor_plan_order(query, actors, carriers);

    for actor_order in actor_order {
        let Ok((entity, id, actor_movement, pos, motion, mut move_intent, mut face_yaw, knockback, _, character)) =
            query.get_mut(actor_order.entity)
        else {
            continue;
        };
        let Some(info) = actors.get(id) else {
            continue;
        };
        let actor_physics = character.0.physics();
        let current_pos = *pos;
        if let Some(anchor) = info.anchor {
            *move_intent = ActorMoveIntent::Idle;
            commands.entity(entity).insert(CharacterSupport::Ground);
            if let ActorMode::Engage { target_pos, .. } = info.mode {
                face_yaw.0 = direction_toward(&current_pos, &target_pos);
            }
            planned_moves.push(CharacterMovePlan::from_target(
                entity,
                current_pos,
                anchor.world_position(carriers),
                0.0,
                actor_physics,
                false,
            ));
            continue;
        }
        let actor_movement = actor_movement.expect("movable actor lacks movement settings");
        // The carriers already advanced to this tick while the actor still
        // stands where the previous pose left it, so that pose maps it into
        // its carrier's frame exactly; the current one would lead it by a
        // tick of carrier travel.
        let pose = carriers.previous_pose(info.carrier);
        let local_pos = pose.inverse_transform_position(&current_pos);
        let move_context = ActorMoveContext {
            entity,
            pos: &current_pos,
            vertical_velocity: motion.0,
            actor_physics,
            delta,
            collision_world,
            planned_moves,
            actor_starts,
            open_barrier_kinds: &plates.open_barrier_kinds,
            map_settings,
            can_use_ladders: character.0.can_use_ladders,
            knockback_step: knockback.map_or(Vec3::ZERO, |velocity| velocity.step(delta)),
            carrier_step: carriers.displacement(info.carrier),
            carriers,
        };

        let mut hold_facing = None;
        let selected = match desired_move(
            info,
            &local_pos,
            &current_pos,
            actor_movement.roam_speed,
            actor_movement.active_speed,
            delta,
        ) {
            ActorDesire::Idle => move_context.idle_move(),
            ActorDesire::HoldFacing { direction } => {
                hold_facing = Some(direction);
                move_context.idle_move()
            }
            ActorDesire::Move { intent, target } => {
                select_route_move(&move_context, intent, &pose.transform_position(&target))
            }
        };

        *move_intent = selected.intent;
        if let Some(direction) = selected.intent.direction().or(hold_facing) {
            face_yaw.0 = direction;
        }
        commands.entity(entity).insert(selected.step.support);
        planned_moves.push(CharacterMovePlan::from_movement_result(
            entity,
            current_pos,
            selected.step,
            actor_physics,
        ));
    }
}

pub(super) fn select_route_move(
    context: &ActorMoveContext<'_>,
    desired: ActorMoveIntent,
    target: &Position,
) -> SelectedActorMove {
    match context.evaluate(desired, target, false) {
        CandidateStep::Clear(selected) | CandidateStep::Graze(selected) => selected,
        CandidateStep::Blocked => context.hold_move(desired),
        CandidateStep::CharacterBlocked => {
            let held = context.hold_move(desired);
            // Grounded ladder approaches must be able to give each other room.
            if desired.uses_ladders() && held.step.support != CharacterSupport::Ground {
                return held;
            }
            let mut direction = desired.direction().expect("direction missing from route movement");
            if desired.uses_ladders()
                && let Some(ladder) = context
                    .collision_world
                    .ladder_volume_at(&Position::from(Vec3::from(*target) + context.carrier_step))
            {
                // Keep opposing approaches moving along the ladder face instead of circling their shared mount.
                let velocity = desired.to_horizontal_velocity();
                let sign = (velocity.x * ladder.normal_x + velocity.z * ladder.normal_z).signum();
                direction = (ladder.normal_x * sign).atan2(ladder.normal_z * sign);
            }
            let speed = desired.speed().expect("speed missing from route movement");
            for sidestep_direction in [direction + FRAC_PI_2, direction - FRAC_PI_2] {
                let sidestep = ActorMoveIntent::Moving {
                    direction: sidestep_direction,
                    speed,
                };
                if let CandidateStep::Clear(selected) = context.evaluate(sidestep, target, true) {
                    return selected;
                }
            }
            held
        }
    }
}
