use bevy::prelude::*;
use common::{
    config::GameplayConfig,
    map::Carriers,
    physics::{
        AirborneMomentum, CharacterMovePlan, CharacterVerticalVelocity, CollisionWorld, KnockbackVelocity,
        PlayerMovementStep, PortalSet, momentum_displacement, player_control_velocity, step_player_movement,
    },
    protocol::{
        ActorMarker, BarrierKindId, MapSettings, PlateState, PlayerId, PlayerMarker, PlayerMoveIntent, Position,
        PowerUpKind,
    },
};

use crate::players::{BumpFeedbackState, LocalPlayerMarker, PlayerAnimationMotion, PlayerMap};

pub(crate) fn plan_player_moves(
    commands: &mut Commands,
    delta: f32,
    collision_world: &CollisionWorld,
    map_settings: &MapSettings,
    gameplay_config: &GameplayConfig,
    players: &PlayerMap,
    plates: &PlateState,
    portal_set: &PortalSet,
    carriers: &Carriers,
    query: &mut PlayerMovementQuery,
    planned_moves: &mut Vec<CharacterMovePlan>,
) {
    let player_physics = gameplay_config.player.physics();
    for (
        entity,
        player_id,
        client_pos,
        move_intent,
        motion,
        _,
        knockback,
        mut airborne_momentum,
        mut animation_motion,
        is_local,
    ) in query
    {
        if !is_local {
            planned_moves.push(CharacterMovePlan::stationary(
                entity,
                *client_pos,
                motion.0,
                player_physics,
            ));
            continue;
        }
        let info = players.get(player_id);
        let has_speed_power_up = info.is_some_and(|i| i.power_up(PowerUpKind::Speed));
        let has_low_gravity = info.is_some_and(|i| i.power_up(PowerUpKind::LowGravity));
        let movement_disabled = info.is_some_and(|i| i.stunned);
        let held_keys: &[BarrierKindId] = info.map_or(&[], |i| i.held_keys.as_slice());

        let control_velocity = player_control_velocity(
            *move_intent,
            &map_settings.movement,
            has_speed_power_up,
            movement_disabled,
        );

        let external_displacement = momentum_displacement(Some(knockback), Some(&*airborne_momentum), delta);
        let step = step_player_movement(PlayerMovementStep {
            start: *client_pos,
            vertical_velocity: motion.0,
            control_velocity,
            delta,
            has_low_gravity,
            held_keys,
            open_kinds: &plates.open_barrier_kinds,
            knockback,
            airborne_momentum: &mut airborne_momentum,
            collision_world,
            map_settings,
            gameplay_config,
            portal_set,
            carriers,
        });
        commands.entity(entity).insert((step.grounding, step.support));
        animation_motion.record_step(*client_pos, &step, control_velocity, external_displacement, delta);
        planned_moves.push(CharacterMovePlan::from_movement_result(
            entity,
            *client_pos,
            step,
            player_physics,
        ));
    }
}

pub(crate) type PlayerMovementQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static PlayerId,
        &'static mut Position,
        &'static PlayerMoveIntent,
        &'static mut CharacterVerticalVelocity,
        Option<&'static mut BumpFeedbackState>,
        &'static KnockbackVelocity,
        &'static mut AirborneMomentum,
        &'static mut PlayerAnimationMotion,
        Has<LocalPlayerMarker>,
    ),
    (With<PlayerMarker>, Without<ActorMarker>),
>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures;
    use bevy::ecs::system::SystemState;
    use common::{
        constants::TICK_SECS,
        protocol::{BarrierKindTable, MapLayout},
    };
    use std::f32::consts::FRAC_PI_2;

    #[test]
    fn remote_bodies_stay_at_reported_positions_while_the_owner_simulates() {
        let gameplay = test_fixtures::gameplay_config();
        let settings = test_fixtures::map_settings();
        let collision = CollisionWorld::from_map_layout(&MapLayout::default(), &BarrierKindTable::default());
        for is_local in [false, true] {
            let mut world = World::new();
            let position = Position {
                x: 0.0,
                y: 10.0,
                z: 0.0,
            };
            let entity = world
                .spawn((
                    PlayerMarker,
                    PlayerId(1),
                    position,
                    PlayerMoveIntent::Walking { direction: FRAC_PI_2 },
                    CharacterVerticalVelocity(-3.0),
                    AirborneMomentum(Vec3::X),
                    KnockbackVelocity(Vec3::X),
                    PlayerAnimationMotion::default(),
                ))
                .id();
            if is_local {
                world.entity_mut(entity).insert(LocalPlayerMarker);
            }
            let mut state = SystemState::<(Commands, PlayerMovementQuery)>::new(&mut world);
            let (mut commands, mut query) = state.get_mut(&mut world).expect("movement query invalid");
            let mut plans = Vec::new();
            plan_player_moves(
                &mut commands,
                TICK_SECS,
                &collision,
                &settings,
                &gameplay,
                &PlayerMap::default(),
                &PlateState::default(),
                &PortalSet::default(),
                &Carriers::default(),
                &mut query,
                &mut plans,
            );
            state.apply(&mut world);
            let plan = plans.first().expect("movement plan missing");
            if is_local {
                assert!(plan.target.x > position.x);
                assert!(plan.target.y < position.y);
            } else {
                assert_eq!(plan.target, position);
                assert_eq!(plan.target_vertical_velocity, -3.0);
                assert_eq!(
                    world
                        .get::<PlayerAnimationMotion>(entity)
                        .expect("animation missing")
                        .velocity,
                    Vec3::ZERO
                );
            }
        }
    }
}
